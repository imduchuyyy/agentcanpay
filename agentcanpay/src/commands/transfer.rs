use std::time::Duration;

use acp_api::{Chain, Client, NATIVE_TOKEN_ADDRESS};
use acp_keystore::Keystore;
use acp_tx::{Status, Transfer};
use acp_wallet::signer_at_path;
use alloy::primitives::Address;

use super::{CommandError, find_chain};
use crate::{cli::TransferArgs, output::Output};

/// Sends value to another address.
///
/// The second command that reads the credential store, and the only one
/// that signs with the key: the phrase is loaded, turned into a signer, and
/// dropped inside this call. It never reaches the browser, stdout or a log.
///
/// Everything a user could be asked about — recipient, token, amount — the
/// agent was already told, so unlike `create` there is no browser step.
pub async fn run(args: &TransferArgs, out: &Output) -> Result<(), CommandError> {
    let to: Address = args
        .to
        .trim()
        .parse()
        .map_err(|_| CommandError::BadAddress(args.to.clone()))?;
    let token = parse_token(args.token.as_deref())?;

    let ks = Keystore::open_default()?;
    let meta = ks.load()?;
    let account = meta
        .account("evm")
        .ok_or_else(|| CommandError::NoAccountForChain("evm".into()))?;

    // The chain listing is what supplies the native symbol and decimals, so
    // a chain whose currency is not 18-decimal ETH is handled by data
    // rather than by assumption.
    let chains = Client::new()?.supported_chains().await?;
    let chain = find_chain(&chains, &args.chain)?;
    if !chain.is_evm() {
        return Err(CommandError::NotEvmChain(chain.name.clone()));
    }
    let rpc_url = endpoint(args, chain)?;

    // Read the secret last, so every avoidable failure happens before the
    // user is asked to unlock anything.
    let phrase = ks.phrase(&meta)?;
    let signer = signer_at_path(&phrase, &account.path)?;
    drop(phrase);

    // A signer that does not match the recorded address would spend from a
    // wallet the caller never asked about.
    if signer.address() != account.address.parse().unwrap_or(Address::ZERO) {
        return Err(CommandError::KeyMismatch);
    }

    out.note(&format!(
        "Sending {} on {} via {rpc_url}…",
        args.amount, chain.name
    ));

    let sent = acp_tx::send(
        signer,
        &rpc_url,
        &Transfer {
            chain_id: chain.chain_id,
            to,
            token,
            amount: args.amount.clone(),
            native_symbol: chain.currency.symbol.clone(),
            native_decimals: chain.currency.decimals,
        },
        (!args.no_wait).then(|| Duration::from_secs(args.timeout)),
    )
    .await?;

    // Printed before the status is judged: a reverted transfer still has a
    // hash, and that hash is what a caller needs to go look at it.
    out.transfer(&sent, &chain.name);

    if sent.status == Status::Reverted {
        return Err(CommandError::Reverted(sent.hash));
    }
    Ok(())
}

/// Resolves `--token` to a contract address, or `None` for native value.
///
/// The native sentinel is accepted as well as omission, because it is what
/// `balance` prints for native holdings and an agent will pass back what it
/// was given.
fn parse_token(token: Option<&str>) -> Result<Option<Address>, CommandError> {
    let Some(raw) = token.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if raw.eq_ignore_ascii_case(NATIVE_TOKEN_ADDRESS) || raw.eq_ignore_ascii_case("native") {
        return Ok(None);
    }
    raw.parse()
        .map(Some)
        .map_err(|_| CommandError::BadAddress(raw.to_owned()))
}

/// The endpoint to broadcast through: whatever was asked for, else the
/// built-in one for this chain.
fn endpoint(args: &TransferArgs, chain: &Chain) -> Result<String, CommandError> {
    if let Some(url) = args
        .rpc_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        return Ok(url.to_owned());
    }
    acp_tx::default_endpoint(chain.chain_id)
        .map(str::to_owned)
        .ok_or(CommandError::Tx(acp_tx::TxError::NoEndpoint(
            chain.chain_id,
        )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";

    #[test]
    fn no_token_means_native_value() {
        assert!(parse_token(None).unwrap().is_none());
        assert!(parse_token(Some("  ")).unwrap().is_none());
    }

    /// `balance` prints the sentinel for native holdings, so passing it
    /// straight back must mean the same thing as omitting `--token`.
    #[test]
    fn the_native_sentinel_round_trips_from_balance_output() {
        assert!(parse_token(Some(NATIVE_TOKEN_ADDRESS)).unwrap().is_none());
        assert!(
            parse_token(Some(
                &NATIVE_TOKEN_ADDRESS.to_uppercase().replace("0X", "0x")
            ))
            .unwrap()
            .is_none()
        );
        assert!(parse_token(Some("native")).unwrap().is_none());
    }

    #[test]
    fn a_contract_address_is_kept_verbatim() {
        let token = parse_token(Some(USDC)).unwrap().unwrap();
        assert_eq!(token.to_string(), USDC);
    }

    /// A mistyped address is a lost transfer, so it fails before any key is
    /// read rather than being coerced into something valid.
    #[test]
    fn a_malformed_token_address_is_rejected() {
        for bad in [
            "0x123",
            "not-an-address",
            "0xzz86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        ] {
            assert!(parse_token(Some(bad)).is_err(), "{bad} should not parse");
        }
    }

    /// Twenty bytes of hex is unambiguous with or without the prefix, and
    /// addresses get pasted both ways.
    #[test]
    fn an_unprefixed_address_is_accepted() {
        let token = parse_token(Some(USDC.trim_start_matches("0x")))
            .unwrap()
            .unwrap();
        assert_eq!(token.to_string(), USDC);
    }
}
