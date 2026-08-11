use alloy::signers::local::{
    MnemonicBuilder, PrivateKeySigner,
    coins_bip39::{English, Entropy, Mnemonic},
};
use zeroize::Zeroizing;

use crate::error::WalletError;

/// Phrase lengths offered to users. Both are standard BIP-39 sizes; 24
/// words carries 256 bits of entropy against 12 words' 128.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCount {
    Twelve,
    TwentyFour,
}

impl WordCount {
    pub fn words(self) -> usize {
        match self {
            WordCount::Twelve => 12,
            WordCount::TwentyFour => 24,
        }
    }
}

fn phrase_of(entropy: Entropy) -> Zeroizing<String> {
    Zeroizing::new(Mnemonic::<English>::new_from_entropy(entropy).to_phrase())
}

/// Generates a new phrase from the operating system's CSPRNG.
///
/// This is the only copy of the key that will ever exist: unlike a
/// signature-derived wallet, nothing can reconstruct it if the phrase and
/// the keystore are both lost.
pub fn generate_phrase(words: WordCount) -> Result<Zeroizing<String>, WalletError> {
    match words {
        WordCount::Twelve => {
            let mut e = Zeroizing::new([0u8; 16]);
            getrandom::fill(e.as_mut_slice()).map_err(|_| WalletError::Rng)?;
            Ok(phrase_of(Entropy::from(*e)))
        }
        WordCount::TwentyFour => {
            let mut e = Zeroizing::new([0u8; 32]);
            getrandom::fill(e.as_mut_slice()).map_err(|_| WalletError::Rng)?;
            Ok(phrase_of(Entropy::from(*e)))
        }
    }
}

/// Turns 32 bytes of entropy into a 24-word BIP-39 phrase.
pub fn phrase_from_entropy(entropy: &[u8; 32]) -> Zeroizing<String> {
    phrase_of(Entropy::from(*entropy))
}

/// Cleans up a pasted phrase before validation.
///
/// Users paste phrases out of password managers and PDFs, so they arrive
/// with newlines, doubled spaces, non-breaking spaces and stray capitals.
/// BIP-39 wordlists are lowercase and single-space separated.
pub fn normalize_phrase(input: &str) -> Zeroizing<String> {
    Zeroizing::new(
        input
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Rejects a phrase whose checksum does not validate.
pub fn validate_phrase(phrase: &str) -> Result<(), WalletError> {
    let words = phrase.split_whitespace().count();
    if !matches!(words, 12 | 15 | 18 | 21 | 24) {
        return Err(WalletError::WordCount(words));
    }
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
    fn generated_phrases_have_the_requested_length_and_validate() {
        for wc in [WordCount::Twelve, WordCount::TwentyFour] {
            let phrase = generate_phrase(wc).unwrap();
            assert_eq!(phrase.split(' ').count(), wc.words());
            validate_phrase(&phrase).unwrap();
        }
    }

    #[test]
    fn generated_phrases_are_not_repeated() {
        let a = generate_phrase(WordCount::TwentyFour).unwrap();
        let b = generate_phrase(WordCount::TwentyFour).unwrap();
        assert_ne!(*a, *b);
    }

    #[test]
    fn normalizes_messy_pasted_input() {
        let messy = "  Abandon\tabandon\nabandon  abandon abandon abandon \
                     abandon abandon abandon abandon abandon ABOUT  ";
        let clean = normalize_phrase(messy);
        assert_eq!(clean.split(' ').count(), 12);
        validate_phrase(&clean).unwrap();
    }

    #[test]
    fn tampered_phrase_is_rejected() {
        let phrase = phrase_from_entropy(&[0u8; 32]).replace("art", "zoo");
        assert!(matches!(
            validate_phrase(&phrase),
            Err(WalletError::Mnemonic)
        ));
    }

    /// A wrong word count is by far the most common paste error, so it gets
    /// its own message rather than a generic checksum failure.
    #[test]
    fn wrong_word_count_is_reported_distinctly() {
        assert!(matches!(
            validate_phrase("abandon abandon abandon"),
            Err(WalletError::WordCount(3))
        ));
    }

    #[test]
    fn a_word_outside_the_wordlist_is_rejected() {
        let phrase = "abandon ".repeat(11) + "notaword";
        assert!(matches!(
            validate_phrase(&phrase),
            Err(WalletError::Mnemonic)
        ));
    }
}
