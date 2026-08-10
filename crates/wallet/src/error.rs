use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("key derivation failed")]
    Kdf,

    #[error("invalid BIP-39 entropy or phrase")]
    Mnemonic,

    #[error("invalid derivation path `{0}`")]
    DerivationPath(String),

    #[error("unknown chain `{0}`")]
    UnknownChain(String),

    #[error("failed to derive signing key")]
    Derive,
}
