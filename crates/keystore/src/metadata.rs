use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Which secret backend holds the phrase for a wallet.
///
/// Persisted so a later run can tell a keychain wallet from a file wallet,
/// and so a silent downgrade to plaintext is impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Keychain,
    File,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Keychain => "keychain",
            Backend::File => "file",
        }
    }
}

/// How the phrase behind this wallet came to exist.
///
/// Recorded because it determines whether the wallet is recoverable
/// without the phrase: only `BrowserSignature` wallets can be re-derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Generated locally from system entropy. The phrase is the only copy.
    Generated,
    /// Imported from a phrase the user already held.
    Imported,
    /// Derived from an external wallet's EIP-712 signature.
    BrowserSignature,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Generated => "generated",
            Source::Imported => "imported",
            Source::BrowserSignature => "browser_signature",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kdf {
    pub alg: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub chain: String,
    pub path: String,
    pub address: String,
}

/// Everything about a wallet except the secret itself.
///
/// `address` reads only this file, so the common agent path never touches
/// the credential store and never prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletMetadata {
    pub version: u32,
    pub id: String,
    pub backend: Backend,
    pub source: Source,
    /// Only set for `BrowserSignature` wallets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorized_by: Option<String>,
    /// Only set for `BrowserSignature` wallets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kdf: Option<Kdf>,
    pub accounts: Vec<Account>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

pub const METADATA_VERSION: u32 = 2;

impl WalletMetadata {
    pub fn account(&self, chain: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.chain == chain)
    }
}
