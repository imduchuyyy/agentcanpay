pub mod chain;
pub mod error;
pub mod kdf;
pub mod seed;

pub use chain::{ChainAccount, Evm};
pub use error::WalletError;
pub use kdf::{KDF_ALG, root_entropy};
pub use seed::{phrase_from_entropy, signer_at_path, validate_phrase};

use alloy::primitives::{Address, Signature};
use zeroize::Zeroizing;

/// A freshly derived wallet: the phrase plus the account addresses that were
/// derived from it.
pub struct DerivedWallet {
    pub phrase: Zeroizing<String>,
    pub accounts: Vec<DerivedAccount>,
}

pub struct DerivedAccount {
    pub chain: &'static str,
    pub path: &'static str,
    pub address: String,
}

/// Full `create` derivation: browser signature to phrase and addresses.
pub fn derive_from_signature(
    sig: &Signature,
    authorized_by: Address,
    index: u32,
    chains: &[&'static dyn ChainAccount],
) -> Result<DerivedWallet, WalletError> {
    let entropy = root_entropy(sig, authorized_by, index)?;
    let phrase = phrase_from_entropy(&entropy);

    let accounts = chains
        .iter()
        .map(|c| {
            Ok(DerivedAccount {
                chain: c.id(),
                path: c.default_path(),
                address: c.address(&phrase, c.default_path())?,
            })
        })
        .collect::<Result<Vec<_>, WalletError>>()?;

    Ok(DerivedWallet { phrase, accounts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::U256;
    use std::str::FromStr;

    #[test]
    fn same_signature_yields_same_wallet() {
        let sig = Signature::new(U256::from(11), U256::from(22), false);
        let who = Address::from_str("0x00000000000000000000000000000000000000ff").unwrap();

        let a = derive_from_signature(&sig, who, 0, chain::SUPPORTED).unwrap();
        let b = derive_from_signature(&sig, who, 0, chain::SUPPORTED).unwrap();

        assert_eq!(*a.phrase, *b.phrase);
        assert_eq!(a.accounts[0].address, b.accounts[0].address);
    }

    #[test]
    fn index_yields_a_different_wallet() {
        let sig = Signature::new(U256::from(11), U256::from(22), false);
        let who = Address::from_str("0x00000000000000000000000000000000000000ff").unwrap();

        let a = derive_from_signature(&sig, who, 0, chain::SUPPORTED).unwrap();
        let b = derive_from_signature(&sig, who, 1, chain::SUPPORTED).unwrap();

        assert_ne!(a.accounts[0].address, b.accounts[0].address);
    }
}
