use crate::{error::WalletError, seed};

/// One supported chain family.
///
/// The seam is deliberately at derivation, not at RPC: a BIP-39 phrase is
/// chain-agnostic, and chains differ by coin type and address encoding. The
/// transaction layer stays EVM-specific until a second chain actually lands.
pub trait ChainAccount: Send + Sync {
    /// Stable identifier persisted in wallet metadata.
    fn id(&self) -> &'static str;

    /// BIP-44 path for the first account of this chain.
    fn default_path(&self) -> &'static str;

    /// Address for `path`, in this chain's canonical text encoding.
    fn address(&self, phrase: &str, path: &str) -> Result<String, WalletError>;
}

pub struct Evm;

impl ChainAccount for Evm {
    fn id(&self) -> &'static str {
        "evm"
    }

    fn default_path(&self) -> &'static str {
        "m/44'/60'/0'/0/0"
    }

    fn address(&self, phrase: &str, path: &str) -> Result<String, WalletError> {
        Ok(seed::signer_at_path(phrase, path)?.address().to_string())
    }
}

pub const SUPPORTED: &[&dyn ChainAccount] = &[&Evm];

pub fn by_id(id: &str) -> Result<&'static dyn ChainAccount, WalletError> {
    SUPPORTED
        .iter()
        .find(|c| c.id() == id)
        .copied()
        .ok_or_else(|| WalletError::UnknownChain(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTOR: &str = "test test test test test test test test test test test junk";

    /// The well-known Foundry/Hardhat default account for this phrase.
    #[test]
    fn evm_derives_the_known_default_account() {
        let addr = Evm.address(VECTOR, Evm.default_path()).unwrap();
        assert_eq!(addr, "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
    }

    #[test]
    fn unknown_chain_is_an_error() {
        assert!(by_id("dogecoin").is_err());
        assert_eq!(by_id("evm").unwrap().id(), "evm");
    }
}
