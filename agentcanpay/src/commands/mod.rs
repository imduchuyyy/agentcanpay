pub mod address;
pub mod create;

use std::process::ExitCode;
use thiserror::Error;

/// Exit codes are part of the agent-facing contract: a caller must be able
/// to tell "no wallet yet" from "user declined" without parsing prose.
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
            Self::Connect(acp_connect::ConnectError::AddressMismatch) => "address_mismatch",
            Self::Connect(_) => "connect",
            Self::Wallet(_) => "wallet",
            Self::NoAccountForChain(_) => "no_account_for_chain",
        }
    }

    pub fn exit_code(&self) -> ExitCode {
        let code: u8 = match self {
            Self::Keystore(acp_keystore::KeystoreError::NoWallet) => 2,
            Self::Connect(
                acp_connect::ConnectError::Timeout | acp_connect::ConnectError::Cancelled(_),
            ) => 3,
            Self::Keystore(
                acp_keystore::KeystoreError::SecretMissing
                | acp_keystore::KeystoreError::NoCredentialStore,
            ) => 4,
            Self::Keystore(acp_keystore::KeystoreError::WalletExists) => 5,
            _ => 1,
        };
        ExitCode::from(code)
    }
}
