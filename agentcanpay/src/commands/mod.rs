pub mod address;
pub mod balance;
pub mod chains;
pub mod create;
pub mod reveal;
pub mod setup;
pub mod store;
pub mod transfer;
pub mod update;

use std::process::ExitCode;

use acp_api::Chain;
use thiserror::Error;

/// Resolves one chain selector, by id or name, against the supported list.
///
/// An unknown selector is an error rather than a miss, so a typo never
/// reads as "that chain holds nothing" or lands a transfer somewhere else.
pub fn find_chain<'a>(chains: &'a [Chain], want: &str) -> Result<&'a Chain, CommandError> {
    chains
        .iter()
        .find(|c| c.chain_id.to_string() == want || c.name.eq_ignore_ascii_case(want))
        .ok_or_else(|| CommandError::UnknownChain(want.to_owned()))
}

/// Exit codes are part of the agent-facing contract: a caller must be able
/// to tell "no wallet yet" from "bad phrase" without parsing prose.
#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Keystore(#[from] acp_keystore::KeystoreError),

    #[error(transparent)]
    Connect(#[from] acp_connect::ConnectError),

    #[error(transparent)]
    Wallet(#[from] acp_wallet::WalletError),

    #[error("wallet has no account for chain `{0}`")]
    NoAccountForChain(String),

    #[error("unknown chain `{0}`; see the supported list")]
    UnknownChain(String),

    #[error(transparent)]
    Api(#[from] acp_api::ApiError),

    #[error(transparent)]
    Tx(#[from] acp_tx::TxError),

    #[error("`{0}` is not a valid address")]
    BadAddress(String),

    #[error("chain `{0}` cannot be used by an Ethereum-style wallet")]
    NotEvmChain(String),

    #[error("the stored key does not match the recorded wallet address")]
    KeyMismatch,

    #[error("transaction {0} reverted; nothing was transferred")]
    Reverted(String),

    #[error("could not check for a newer release: {0}")]
    UpdateCheck(String),

    #[error("this binary is managed by {0}; update it with that instead")]
    UpdateManaged(String),

    #[error("update failed: {0}")]
    UpdateFailed(String),

    #[error("could not install the skill: {0}")]
    Setup(String),
}

impl CommandError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Keystore(acp_keystore::KeystoreError::NoWallet) => "no_wallet",
            Self::Keystore(acp_keystore::KeystoreError::WalletExists) => "wallet_exists",
            Self::Keystore(acp_keystore::KeystoreError::SecretMissing) => "secret_missing",
            Self::Keystore(acp_keystore::KeystoreError::NoCredentialStore) => "no_credential_store",
            Self::Keystore(_) => "keystore",
            Self::Connect(acp_connect::ConnectError::Timeout) => "timeout",
            Self::Connect(acp_connect::ConnectError::Cancelled(_)) => "cancelled",
            Self::Connect(acp_connect::ConnectError::Browser) => "needs_browser",
            Self::Connect(_) => "connect",
            Self::Wallet(
                acp_wallet::WalletError::Mnemonic | acp_wallet::WalletError::WordCount(_),
            ) => "invalid_phrase",
            Self::Wallet(_) => "wallet",
            Self::NoAccountForChain(_) => "no_account_for_chain",
            Self::UnknownChain(_) => "unknown_chain",
            Self::Api(_) => "api",
            Self::Tx(e) => tx_kind(e),
            Self::BadAddress(_) => "invalid_address",
            Self::NotEvmChain(_) => "unusable_chain",
            Self::KeyMismatch => "key_mismatch",
            Self::Reverted(_) => "reverted",
            Self::UpdateCheck(_) => "update_check",
            Self::UpdateManaged(_) => "update_managed",
            Self::UpdateFailed(_) => "update_failed",
            Self::Setup(_) => "setup_failed",
        }
    }

    pub fn exit_code(&self) -> ExitCode {
        let code: u8 = match self {
            Self::Keystore(acp_keystore::KeystoreError::NoWallet) => 2,
            // 3 means "the user did not supply a usable phrase" — whether
            // they cancelled, ran out of time, or typed it wrong.
            Self::Connect(
                acp_connect::ConnectError::Timeout | acp_connect::ConnectError::Cancelled(_),
            )
            | Self::Wallet(
                acp_wallet::WalletError::Mnemonic | acp_wallet::WalletError::WordCount(_),
            ) => 3,
            Self::Keystore(
                acp_keystore::KeystoreError::SecretMissing
                | acp_keystore::KeystoreError::NoCredentialStore,
            ) => 4,
            Self::Keystore(acp_keystore::KeystoreError::WalletExists) => 5,
            // 6 is "the outside world did not cooperate" — retryable, and
            // distinct from anything wrong with the wallet itself.
            Self::Api(_) | Self::Tx(acp_tx::TxError::Rpc(_)) => 6,
            // 7 is "no value moved", whatever the reason. A caller that
            // sees it can be certain the transfer did not happen — except
            // for a revert, which consumed gas and is reported as such.
            Self::Tx(_) | Self::BadAddress(_) | Self::Reverted(_) => 7,
            // 8 is "this binary was not replaced", for every reason. The
            // wallet is untouched and the caller's fallback is the same
            // either way: run the install script, which is the code path
            // `update` drives anyway.
            Self::UpdateCheck(_) | Self::UpdateManaged(_) | Self::UpdateFailed(_) => 8,
            // 9 is "the skill was not written". The wallet works either
            // way; only an agent's ability to discover it is affected.
            Self::Setup(_) => 9,
            _ => 1,
        };
        ExitCode::from(code)
    }
}

fn tx_kind(e: &acp_tx::TxError) -> &'static str {
    use acp_tx::TxError as T;
    match e {
        T::NoEndpoint(_) => "no_rpc_endpoint",
        T::BadUrl(_) => "invalid_rpc_url",
        T::ChainMismatch { .. } => "chain_mismatch",
        T::Rpc(_) => "rpc",
        T::BadAmount(_) | T::ZeroAmount => "invalid_amount",
        T::NotAToken(_) => "not_a_token",
        T::InsufficientFunds { .. } | T::InsufficientForGas { .. } => "insufficient_funds",
        T::Rejected(_) => "rejected",
    }
}
