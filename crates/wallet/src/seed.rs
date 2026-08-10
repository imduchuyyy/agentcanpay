use alloy::signers::local::{
    MnemonicBuilder, PrivateKeySigner,
    coins_bip39::{English, Mnemonic},
};
use zeroize::Zeroizing;

use crate::error::WalletError;

/// Turns 32 bytes of entropy into a 24-word BIP-39 phrase.
///
/// A mnemonic rather than a raw key so the user can import the agent wallet
/// into an ordinary wallet app if they ever need to.
pub fn phrase_from_entropy(entropy: &[u8; 32]) -> Zeroizing<String> {
    let mnemonic = Mnemonic::<English>::new_from_entropy((*entropy).into());
    Zeroizing::new(mnemonic.to_phrase())
}

/// Rejects a phrase whose checksum does not validate.
pub fn validate_phrase(phrase: &str) -> Result<(), WalletError> {
    Mnemonic::<English>::new_from_phrase(phrase)
        .map(|_| ())
        .map_err(|_| WalletError::Mnemonic)
}

/// Builds the signing key for `path` from a BIP-39 phrase.
pub fn signer_at_path(phrase: &str, path: &str) -> Result<PrivateKeySigner, WalletError> {
    MnemonicBuilder::<English>::default()
        .phrase(phrase)
        .derivation_path(path)
        .map_err(|_| WalletError::DerivationPath(path.to_owned()))?
        .build()
        .map_err(|_| WalletError::Derive)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical BIP-39 vector: 32 zero bytes encode to a 24-word phrase
    /// ending in "art".
    #[test]
    fn zero_entropy_matches_bip39_vector() {
        let phrase = phrase_from_entropy(&[0u8; 32]);
        let words: Vec<&str> = phrase.split(' ').collect();
        assert_eq!(words.len(), 24);
        assert!(words[..23].iter().all(|w| *w == "abandon"));
        assert_eq!(words[23], "art");
    }

    #[test]
    fn generated_phrases_round_trip() {
        let phrase = phrase_from_entropy(&[0x42u8; 32]);
        validate_phrase(&phrase).unwrap();
    }

    #[test]
    fn tampered_phrase_is_rejected() {
        let phrase = phrase_from_entropy(&[0u8; 32]).replace("art", "zoo");
        assert!(validate_phrase(&phrase).is_err());
    }
}
