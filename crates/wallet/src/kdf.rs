use alloy::primitives::{Address, Signature, U256, uint};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::WalletError;

/// Identifier recorded in wallet metadata so a future scheme can be told
/// apart from this one during re-derivation.
pub const KDF_ALG: &str = "hkdf-sha256-v1";

const HKDF_SALT: &[u8] = b"agentcanpay-seed-v1";

const SECP256K1_N: U256 =
    uint!(0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141_U256);

/// Canonical 64-byte `r‖s` for a signature, with `s` folded into the lower
/// half of the curve order.
///
/// The recovery id is deliberately excluded: wallets disagree on whether it
/// is encoded as 27/28 or 0/1, so including it would make the derived wallet
/// depend on which wallet app the user happened to sign with. Folding `s`
/// covers the same hazard for malleable signatures.
fn canonical_rs(sig: &Signature) -> Zeroizing<[u8; 64]> {
    let s = sig.s();
    let s = if s > SECP256K1_N >> 1 {
        SECP256K1_N - s
    } else {
        s
    };

    let mut out = Zeroizing::new([0u8; 64]);
    out[..32].copy_from_slice(&sig.r().to_be_bytes::<32>());
    out[32..].copy_from_slice(&s.to_be_bytes::<32>());
    out
}

/// Derives BIP-39 entropy from the browser wallet's EIP-712 signature.
///
/// `authorized_by` and `index` are bound into the HKDF info string, so one
/// browser key yields a distinct agent wallet per index. Formatting is
/// lowercase hex written out by hand rather than via `Display`, because this
/// string is consensus-critical: any change to it silently orphans every
/// wallet already created.
pub fn root_entropy(
    sig: &Signature,
    authorized_by: Address,
    index: u32,
) -> Result<Zeroizing<[u8; 32]>, WalletError> {
    let ikm = canonical_rs(sig);
    let info = format!("eip712:0x{}:{index}", hex::encode(authorized_by.as_slice()));

    let mut entropy = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(HKDF_SALT), ikm.as_slice())
        .expand(info.as_bytes(), entropy.as_mut_slice())
        .map_err(|_| WalletError::Kdf)?;
    Ok(entropy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn addr() -> Address {
        Address::from_str("0x00000000000000000000000000000000000000ff").unwrap()
    }

    fn sig(r: U256, s: U256, parity: bool) -> Signature {
        Signature::new(r, s, parity)
    }

    /// Frozen output: a change here means every existing wallet would fail
    /// to re-derive, so this test must only ever be updated alongside a
    /// `KDF_ALG` version bump.
    #[test]
    fn entropy_is_stable() {
        let s = sig(U256::from(1), U256::from(2), false);
        let got = root_entropy(&s, addr(), 0).unwrap();
        assert_eq!(
            hex::encode(*got),
            "c3621b5cb50f48a5ae363769a05b072e2592817ac6dd53e2c824f5851cd8d8bf"
        );
    }

    #[test]
    fn high_s_folds_to_the_same_entropy() {
        let low = sig(U256::from(7), U256::from(9), false);
        let high = sig(U256::from(7), SECP256K1_N - U256::from(9), false);
        assert_eq!(
            *root_entropy(&low, addr(), 0).unwrap(),
            *root_entropy(&high, addr(), 0).unwrap()
        );
    }

    #[test]
    fn recovery_id_does_not_affect_entropy() {
        let a = sig(U256::from(7), U256::from(9), false);
        let b = sig(U256::from(7), U256::from(9), true);
        assert_eq!(
            *root_entropy(&a, addr(), 0).unwrap(),
            *root_entropy(&b, addr(), 0).unwrap()
        );
    }

    #[test]
    fn index_and_account_separate_wallets() {
        let s = sig(U256::from(7), U256::from(9), false);
        let other = Address::from_str("0x00000000000000000000000000000000000000ee").unwrap();
        assert_ne!(
            *root_entropy(&s, addr(), 0).unwrap(),
            *root_entropy(&s, addr(), 1).unwrap()
        );
        assert_ne!(
            *root_entropy(&s, addr(), 0).unwrap(),
            *root_entropy(&s, other, 0).unwrap()
        );
    }
}
