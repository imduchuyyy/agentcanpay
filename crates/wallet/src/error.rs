use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("key derivation failed")]
    Kdf,

    #[error("recovery phrase is not valid BIP-39: check for typos or reordered words")]
    Mnemonic,

    #[error("expected 12, 15, 18, 21 or 24 words, got {0}")]
    WordCount(usize),

    #[error("system random number generator unavailable")]
    Rng,

    #[error("invalid derivation path `{0}`")]
    DerivationPath(String),

    #[error("unknown chain `{0}`")]
    UnknownChain(String),

    #[error("failed to derive signing key")]
    Derive,
}
