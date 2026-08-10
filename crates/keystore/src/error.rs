use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeystoreError {
    #[error("no wallet found; run `agentcanpay create` first")]
    NoWallet,

    #[error("a wallet already exists; pass --force to replace it")]
    WalletExists,

    #[error("wallet metadata is present but its secret is missing from the store")]
    SecretMissing,

    #[error("no OS credential store available; re-run with --keystore file")]
    NoCredentialStore,

    #[error("credential store error: {0}")]
    Keychain(String),

    #[error("unsupported wallet metadata version {0}")]
    UnsupportedVersion(u32),

    #[error("could not determine home directory; set AGENTCANPAY_HOME")]
    NoHome,

    #[error("invalid keystore path")]
    BadPath,

    #[error("wallet metadata is corrupt: {0}")]
    Corrupt(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
