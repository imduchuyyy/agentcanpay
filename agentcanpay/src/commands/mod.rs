pub mod address;
pub mod create;
pub mod import;
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
    Wallet(#[from] acp_wallet::WalletError),

    #[error("wallet has no account for chain `{0}`")]
    NoAccountForChain(String),

    #[error("no recovery phrase on stdin")]
    EmptyPhrase,

    #[error("could not read the recovery phrase: {0}")]
    ReadPhrase(#[source] std::io::Error),
}

impl CommandError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Keystore(acp_keystore::KeystoreError::NoWallet) => "no_wallet",
            Self::Keystore(acp_keystore::KeystoreError::WalletExists) => "wallet_exists",
            Self::Keystore(acp_keystore::KeystoreError::SecretMissing) => "secret_missing",
            Self::Keystore(acp_keystore::KeystoreError::NoCredentialStore) => "no_credential_store",
            Self::Keystore(_) => "keystore",
            Self::Wallet(
                acp_wallet::WalletError::Mnemonic | acp_wallet::WalletError::WordCount(_),
            ) => "invalid_phrase",
            Self::Wallet(_) => "wallet",
            Self::NoAccountForChain(_) => "no_account_for_chain",
            Self::EmptyPhrase | Self::ReadPhrase(_) => "phrase_input",
        }
    }

    pub fn exit_code(&self) -> ExitCode {
        let code: u8 = match self {
            Self::Keystore(acp_keystore::KeystoreError::NoWallet) => 2,
            Self::Wallet(
                acp_wallet::WalletError::Mnemonic | acp_wallet::WalletError::WordCount(_),
            )
            | Self::EmptyPhrase
            | Self::ReadPhrase(_) => 3,
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
