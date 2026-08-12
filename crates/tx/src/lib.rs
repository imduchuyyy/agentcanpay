//! Signs and broadcasts EVM transfers.
//!
//! Balances are read through `acp-api`, which needs no RPC endpoint. Moving
//! value does: a transaction has to reach a node. That is the whole reason
//! this crate exists separately — it is the only part of the project that
//! talks to a chain rather than to an API.

pub mod error;
pub mod rpc;

pub use error::TxError;
pub use rpc::{chains_with_endpoints, default_endpoint};

use std::time::Duration;

use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{Address, U256, utils::format_units, utils::parse_units},
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    sol,
};

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function decimals() external view returns (uint8);
        function symbol() external view returns (string);
        function balanceOf(address owner) external view returns (uint256);
        function transfer(address to, uint256 value) external returns (bool);
    }
}

/// What to send, in the terms a caller has them in.
///
/// `amount` stays a decimal string all the way here: scaling it needs the
/// token's decimals, which are read on-chain for an ERC-20 and supplied by
/// the caller for native value.
pub struct Transfer {
    pub chain_id: u64,
    pub to: Address,
    /// `None` sends the chain's native currency.
    pub token: Option<Address>,
    /// Whole units as a human would write them, e.g. `"1.5"`.
    pub amount: String,
    /// Native currency, used only when `token` is `None`. Comes from the
    /// chain listing rather than being assumed to be 18-decimal ETH.
    pub native_symbol: String,
    pub native_decimals: u8,
}

/// How far a transaction got.
///
/// `Pending` is a real outcome, not a failure: a broadcast transaction with
/// a hash will usually still land, so the caller gets the hash either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Success,
    Reverted,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Success => "success",
            Status::Reverted => "reverted",
        }
    }
}

/// What was sent, and what became of it.
pub struct Sent {
    pub chain_id: u64,
    pub hash: String,
    pub from: Address,
    pub to: Address,
    /// `None` for native value; the caller renders that as it prefers.
    pub token: Option<Address>,
    pub symbol: String,
    pub decimals: u8,
    /// The amount as accepted, re-rendered from the integer actually sent.
    pub amount: String,
    /// The same amount in the token's smallest unit. A string because it
    /// routinely exceeds what an IEEE double holds exactly.
    pub raw_amount: String,
    pub status: Status,
    pub block: Option<u64>,
    pub gas_used: Option<u64>,
}

impl Sent {
    pub fn is_native(&self) -> bool {
        self.token.is_none()
    }
}

/// Scales a human amount by `decimals`.
///
/// Stricter than `parse_units` on both ends, because both of its lenient
/// behaviours move the wrong amount of money: a negative parses into a
/// signed value that would convert to an enormous `U256`, and extra
/// precision is silently truncated, which turns 1.0000001 USDC into 1.
pub fn scale_amount(amount: &str, decimals: u8) -> Result<U256, TxError> {
    let trimmed = amount.trim();
    let bad = || TxError::BadAmount(amount.to_owned());

    if trimmed.starts_with('-') {
        return Err(bad());
    }
    if let Some((_, fraction)) = trimmed.split_once('.')
        && fraction.len() > decimals as usize
    {
        return Err(bad());
    }

    let parsed = parse_units(trimmed, decimals)
        .map_err(|_| bad())?
        .get_absolute();
    if parsed == U256::ZERO {
        return Err(TxError::ZeroAmount);
    }
    Ok(parsed)
}

/// Renders a raw amount back into whole units, trimmed.
fn human(raw: U256, decimals: u8) -> String {
    let rendered = format_units(raw, decimals).unwrap_or_else(|_| raw.to_string());
    match rendered.split_once('.') {
        Some(_) => rendered
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned(),
        None => rendered,
    }
}

fn rpc_err(e: impl std::fmt::Display) -> TxError {
    TxError::Rpc(e.to_string())
}

/// Signs and broadcasts a transfer.
///
/// The signer is taken by value: this is the only place in the project
/// besides `reveal` that holds key material, and it should not outlive the
/// call. `wait` bounds how long to watch for a receipt; `None` returns as
/// soon as the transaction is accepted by the node.
pub async fn send(
    signer: PrivateKeySigner,
    rpc_url: &str,
    transfer: &Transfer,
    wait: Option<Duration>,
) -> Result<Sent, TxError> {
    let url = rpc_url
        .parse()
        .map_err(|_| TxError::BadUrl(rpc_url.to_owned()))?;
    let from = signer.address();
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(signer))
        .connect_http(url);

    // Before anything is signed: a transaction built for one chain is a
    // perfectly valid one to submit on another.
    let actual = provider.get_chain_id().await.map_err(rpc_err)?;
    if actual != transfer.chain_id {
        return Err(TxError::ChainMismatch {
            expected: transfer.chain_id,
            actual,
        });
    }

    let (symbol, decimals) = match transfer.token {
        None => (transfer.native_symbol.clone(), transfer.native_decimals),
        Some(token) => token_metadata(&provider, token).await?,
    };
    let value = scale_amount(&transfer.amount, decimals)?;

    let pending = match transfer.token {
        None => send_native(&provider, transfer.to, value, from, &symbol, decimals).await?,
        Some(token) => {
            send_erc20(
                &provider,
                token,
                transfer.to,
                value,
                from,
                &symbol,
                decimals,
            )
            .await?
        }
    };

    let hash = pending.tx_hash().to_string();
    let (status, block, gas_used) = confirm(pending, wait).await?;

    Ok(Sent {
        chain_id: transfer.chain_id,
        hash,
        from,
        to: transfer.to,
        token: transfer.token,
        symbol,
        decimals,
        amount: human(value, decimals),
        raw_amount: value.to_string(),
        status,
        block,
        gas_used,
    })
}

/// Reads a token's own symbol and decimals.
///
/// Decimals are mandatory — without them the requested amount cannot be
/// scaled, and guessing 18 would send a thousand times too much on a USDC
/// transfer. A missing symbol is only cosmetic, so it falls back.
async fn token_metadata<P: Provider>(
    provider: &P,
    token: Address,
) -> Result<(String, u8), TxError> {
    let erc20 = IERC20::new(token, provider);
    let decimals = erc20
        .decimals()
        .call()
        .await
        .map_err(|_| TxError::NotAToken(token.to_string()))?;
    let symbol = erc20
        .symbol()
        .call()
        .await
        .unwrap_or_else(|_| token.to_string());
    Ok((symbol, decimals))
}

async fn send_native<P: Provider>(
    provider: &P,
    to: Address,
    value: U256,
    from: Address,
    symbol: &str,
    decimals: u8,
) -> Result<PendingTransactionBuilder<alloy::network::Ethereum>, TxError> {
    let balance = provider.get_balance(from).await.map_err(rpc_err)?;
    if balance < value {
        return Err(TxError::InsufficientFunds {
            symbol: symbol.to_owned(),
            have: human(balance, decimals),
            want: human(value, decimals),
        });
    }

    let request = TransactionRequest::default()
        .with_from(from)
        .with_to(to)
        .with_value(value);

    // Native value competes with gas, so the balance covering the amount is
    // not enough. Best-effort: chains that price gas unusually just fall
    // through to the node's own rejection.
    if let Some(fee) = estimated_fee(provider, &request).await
        && balance < value + fee
    {
        return Err(TxError::InsufficientForGas {
            symbol: symbol.to_owned(),
            have: human(balance, decimals),
            want: human(value, decimals),
            fee: human(fee, decimals),
        });
    }

    provider
        .send_transaction(request)
        .await
        .map_err(|e| TxError::Rejected(e.to_string()))
}

async fn send_erc20<P: Provider>(
    provider: &P,
    token: Address,
    to: Address,
    value: U256,
    from: Address,
    symbol: &str,
    decimals: u8,
) -> Result<PendingTransactionBuilder<alloy::network::Ethereum>, TxError> {
    let erc20 = IERC20::new(token, provider);

    // Checked here because an ERC-20 transfer that overdraws reverts, which
    // costs gas and reports nothing useful about why.
    let balance = erc20.balanceOf(from).call().await.map_err(rpc_err)?;
    if balance < value {
        return Err(TxError::InsufficientFunds {
            symbol: symbol.to_owned(),
            have: human(balance, decimals),
            want: human(value, decimals),
        });
    }

    erc20
        .transfer(to, value)
        .send()
        .await
        .map_err(|e| TxError::Rejected(e.to_string()))
}

/// Gas cost of `request` at current fees, or `None` if the chain does not
/// answer either question in the expected shape.
async fn estimated_fee<P: Provider>(provider: &P, request: &TransactionRequest) -> Option<U256> {
    let gas = provider.estimate_gas(request.clone()).await.ok()?;
    let fees = provider.estimate_eip1559_fees().await.ok()?;
    Some(U256::from(gas) * U256::from(fees.max_fee_per_gas))
}

/// Watches for a receipt, if asked to.
///
/// A timeout is not an error: the transaction is already broadcast, and the
/// caller needs the hash more than it needs a status.
async fn confirm(
    pending: PendingTransactionBuilder<alloy::network::Ethereum>,
    wait: Option<Duration>,
) -> Result<(Status, Option<u64>, Option<u64>), TxError> {
    let Some(timeout) = wait else {
        return Ok((Status::Pending, None, None));
    };

    match tokio::time::timeout(timeout, pending.get_receipt()).await {
        Err(_elapsed) => Ok((Status::Pending, None, None)),
        Ok(Err(e)) => Err(TxError::Rpc(e.to_string())),
        Ok(Ok(receipt)) => {
            let status = if receipt.status() {
                Status::Success
            } else {
                Status::Reverted
            };
            Ok((status, receipt.block_number, Some(receipt.gas_used)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_by_the_tokens_own_decimals() {
        assert_eq!(
            scale_amount("1.5", 18).unwrap(),
            U256::from(1_500_000_000_000_000_000u64)
        );
        assert_eq!(scale_amount("1.5", 6).unwrap(), U256::from(1_500_000));
        assert_eq!(scale_amount("1", 0).unwrap(), U256::from(1));
    }

    /// Whitespace survives shell quoting and copy-paste; a wrong unit does
    /// not, and must not be guessed at.
    #[test]
    fn rejects_amounts_that_are_not_plain_numbers() {
        assert!(scale_amount(" 2.5 ", 18).is_ok());
        for bad in ["", "abc", "1.5 ETH", "1,5", "-1"] {
            assert!(scale_amount(bad, 18).is_err(), "{bad} should not parse");
        }
    }

    /// More decimals than the token has would silently truncate, which on a
    /// 6-decimal stablecoin turns 1.0000001 into 1.
    #[test]
    fn rejects_more_precision_than_the_token_has() {
        assert!(scale_amount("1.0000001", 6).is_err());
    }

    #[test]
    fn zero_is_rejected_rather_than_broadcast() {
        assert!(matches!(scale_amount("0", 18), Err(TxError::ZeroAmount)));
        assert!(matches!(
            scale_amount("0.000", 18),
            Err(TxError::ZeroAmount)
        ));
    }

    #[test]
    fn renders_amounts_back_without_padding() {
        assert_eq!(human(U256::from(1_500_000_000_000_000_000u64), 18), "1.5");
        assert_eq!(human(U256::from(1_000_000), 6), "1");
        assert_eq!(human(U256::ZERO, 18), "0");
    }

    /// A round trip must not change the quantity: this is the number the
    /// user is shown after their money has moved.
    #[test]
    fn scaling_round_trips() {
        for (amount, decimals) in [("1.5", 18), ("0.000001", 6), ("12345.6789", 8)] {
            let raw = scale_amount(amount, decimals).unwrap();
            assert_eq!(human(raw, decimals), amount);
        }
    }
}
