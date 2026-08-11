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
