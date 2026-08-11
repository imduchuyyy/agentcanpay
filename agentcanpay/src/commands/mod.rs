pub mod address;
pub mod balance;
pub mod create;
pub mod reveal;
pub mod store;

use std::process::ExitCode;
use thiserror::Error;

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
            Self::Api(_) => 6,
            _ => 1,
        };
        ExitCode::from(code)
    }
}
