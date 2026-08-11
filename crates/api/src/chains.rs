use serde::Deserialize;

/// The native currency of a chain.
///
/// Socket represents it as a pseudo-token at `0xeeee…eeee`, and it appears
/// under that address in token lists too.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Currency {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub min_native_currency_for_gas: Option<String>,
}

/// A chain Socket can route through.
///
/// Fields beyond these exist upstream (`dexes`, `bridges`, `explorers`) and
/// are ignored rather than modelled, so a new one appearing cannot break
/// deserialization.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chain {
    pub chain_id: u64,
    pub name: String,
    pub currency: Currency,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub sending_enabled: bool,
    #[serde(default)]
    pub receiving_enabled: bool,
}

/// Chains that do not use Ethereum-style addresses.
///
/// Maintained by hand because the API offers nothing to derive it from:
/// Solana, Tron, Stellar and Sui all report the same `0xeeee…eeee` sentinel
/// as their native currency address, exactly as every EVM chain does, and
/// only Bitcoin happens to differ. Revisit when Socket adds a chain — a new
/// non-EVM one would otherwise be reported as usable by an EVM wallet.
const NON_EVM_CHAIN_IDS: &[u64] = &[
    89_999,      // Solana
    728_126_428, // Tron
    1_110_002,   // Stellar
    1_110_006,   // Sui
    8_253_038,   // Bitcoin
];

impl Chain {
    /// Whether an Ethereum-style address can hold anything here.
    pub fn is_evm(&self) -> bool {
        !NON_EVM_CHAIN_IDS.contains(&self.chain_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(chain_id: u64) -> Chain {
        serde_json::from_value(serde_json::json!({
            "chainId": chain_id,
            "name": "Test",
            "currency": {
                "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                "name": "Test", "symbol": "TST", "decimals": 18
            }
        }))
        .unwrap()
    }

    #[test]
    fn evm_chains_are_usable_by_an_evm_wallet() {
        for id in [1, 10, 8453, 42_161, 1337] {
            assert!(chain(id).is_evm(), "{id} should be EVM");
        }
    }

    #[test]
    fn known_non_evm_chains_are_excluded() {
        for id in [89_999, 728_126_428, 1_110_002, 1_110_006, 8_253_038] {
            assert!(!chain(id).is_evm(), "{id} should not be EVM");
        }
    }
}
