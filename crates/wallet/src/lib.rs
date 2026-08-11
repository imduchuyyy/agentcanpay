pub mod chain;
pub mod error;
pub mod kdf;
pub mod seed;

pub use chain::{ChainAccount, Evm};
pub use error::WalletError;
pub use kdf::{KDF_ALG, root_entropy};
pub use seed::{
    WordCount, generate_phrase, normalize_phrase, phrase_from_entropy, signer_at_path,
    validate_phrase,
};

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

/// Derives every supported chain's account from an existing phrase.
///
/// The phrase is validated first, so an imported typo fails here rather
/// than silently producing a wallet the user cannot fund.
pub fn derive_from_phrase(
    phrase: Zeroizing<String>,
    chains: &[&'static dyn ChainAccount],
) -> Result<DerivedWallet, WalletError> {
    validate_phrase(&phrase)?;

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

/// Generates a brand-new wallet from system entropy.
pub fn create_wallet(
    words: WordCount,
    chains: &[&'static dyn ChainAccount],
) -> Result<DerivedWallet, WalletError> {
    derive_from_phrase(generate_phrase(words)?, chains)
}

/// Full `create` derivation from a browser signature.
///
/// Retained for the external-wallet authorisation flow in `acp-connect`;
/// the CLI's `create` now generates a phrase locally instead.
pub fn derive_from_signature(
    sig: &Signature,
    authorized_by: Address,
    index: u32,
    chains: &[&'static dyn ChainAccount],
) -> Result<DerivedWallet, WalletError> {
    let entropy = root_entropy(sig, authorized_by, index)?;
    derive_from_phrase(phrase_from_entropy(&entropy), chains)
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

    #[test]
    fn created_wallets_are_distinct_and_reimportable() {
        let a = create_wallet(WordCount::TwentyFour, chain::SUPPORTED).unwrap();
        let b = create_wallet(WordCount::TwentyFour, chain::SUPPORTED).unwrap();
        assert_ne!(a.accounts[0].address, b.accounts[0].address);

        // Importing what `create` produced must land on the same address.
        let again = derive_from_phrase(a.phrase.clone(), chain::SUPPORTED).unwrap();
        assert_eq!(again.accounts[0].address, a.accounts[0].address);
    }

    #[test]
    fn importing_a_known_phrase_gives_the_known_address() {
        let phrase = Zeroizing::new(
            "test test test test test test test test test test test junk".to_owned(),
        );
        let w = derive_from_phrase(phrase, chain::SUPPORTED).unwrap();
        assert_eq!(
            w.accounts[0].address,
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        );
    }

    #[test]
    fn importing_a_bad_phrase_fails_before_deriving() {
        let phrase = Zeroizing::new("not a valid recovery phrase at all".to_owned());
        assert!(derive_from_phrase(phrase, chain::SUPPORTED).is_err());
    }
}
