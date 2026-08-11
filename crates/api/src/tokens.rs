use alloy::primitives::{U256, utils::format_units};
use serde::Deserialize;

/// Which token list to request.
///
/// `Full` is upwards of fifty thousand tokens per chain; `Trending` is a
/// few hundred and is what a balance view wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Trending,
    Full,
}

impl ListKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ListKind::Trending => "trending",
            ListKind::Full => "full",
        }
    }
}

/// A token, plus the requested address's holding of it.
///
/// Nearly everything is optional: the upstream response returns `null` for
/// unranked or unpriced tokens, and a strict struct would fail the whole
/// request over one of them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub chain_id: u64,
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(default)]
    pub logo_uri: Option<String>,
    #[serde(default)]
    pub is_verified: Option<bool>,
    #[serde(default)]
    pub trending_rank: Option<u64>,
    #[serde(default)]
    pub market_cap: Option<f64>,
    /// Raw integer amount, unscaled by `decimals`.
    #[serde(default)]
    pub balance: Option<String>,
    #[serde(default)]
    pub balance_in_usd: Option<f64>,
}

impl Token {
    /// The raw holding. An unparseable or absent balance reads as zero
    /// rather than failing: one malformed token should not sink a listing.
    pub fn raw_balance(&self) -> U256 {
        self.balance
            .as_deref()
            .and_then(|b| b.parse::<U256>().ok())
            .unwrap_or(U256::ZERO)
    }

    pub fn has_balance(&self) -> bool {
        self.raw_balance() > U256::ZERO
    }

    /// The holding scaled by `decimals`, for display.
    ///
    /// Trailing zeros are trimmed: `format_units` pads to full precision,
    /// which renders a balance as `6.634031000000000000`.
    pub fn amount(&self) -> String {
        let raw =
            format_units(self.raw_balance(), self.decimals).unwrap_or_else(|_| "0".to_owned());
        match raw.split_once('.') {
            Some(_) => raw.trim_end_matches('0').trim_end_matches('.').to_owned(),
            None => raw,
        }
    }

    pub fn usd(&self) -> f64 {
        self.balance_in_usd.unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(balance: Option<&str>, decimals: u8) -> Token {
        Token {
            chain_id: 1,
            address: "0x0".into(),
            name: "Test".into(),
            symbol: "TST".into(),
            decimals,
            logo_uri: None,
            is_verified: None,
            trending_rank: None,
            market_cap: None,
            balance: balance.map(str::to_owned),
            balance_in_usd: None,
        }
    }

    #[test]
    fn scales_by_decimals() {
        assert_eq!(token(Some("6634031000000000000"), 18).amount(), "6.634031");
        assert_eq!(token(Some("1500000"), 6).amount(), "1.5");
        assert_eq!(token(Some("1000000"), 6).amount(), "1");
        assert_eq!(token(Some("0"), 6).amount(), "0");
    }

    /// Balances routinely exceed u64: a token with 18 decimals and ten
    /// billion units is 1e28.
    #[test]
    fn handles_balances_far_beyond_u64() {
        let t = token(Some("10000000000000000000000000000"), 18);
        assert!(t.has_balance());
        assert_eq!(t.amount(), "10000000000");
    }

    #[test]
    fn absent_or_malformed_balances_read_as_zero() {
        assert!(!token(None, 18).has_balance());
        assert!(!token(Some(""), 18).has_balance());
        assert!(!token(Some("not-a-number"), 18).has_balance());
        assert!(!token(Some("0"), 18).has_balance());
    }
}
