/// Where to broadcast, per chain.
///
/// Maintained by hand, like `acp_api::chains::NON_EVM_CHAIN_IDS`, because
/// the Socket API exposes routing data but no RPC endpoints. These are the
/// public endpoints each chain documents; they rate-limit, so `--rpc-url`
/// overrides any of them. A chain missing here is not unsupported — it
/// just needs an endpoint passed in.
///
/// Every entry was checked against `eth_chainId` when added. Add a chain
/// only after doing the same, since a wrong endpoint here would be caught
/// by the chain-id guard in `send` and read as an outage.
const DEFAULT_ENDPOINTS: &[(u64, &str)] = &[
    (1, "https://ethereum-rpc.publicnode.com"),
    (10, "https://optimism-rpc.publicnode.com"),
    (56, "https://bsc-rpc.publicnode.com"),
    (100, "https://gnosis-rpc.publicnode.com"),
    (130, "https://unichain-rpc.publicnode.com"),
    (137, "https://polygon-bor-rpc.publicnode.com"),
    (146, "https://sonic-rpc.publicnode.com"),
    (324, "https://mainnet.era.zksync.io"),
    (480, "https://worldchain-mainnet.g.alchemy.com/public"),
    (999, "https://rpc.hyperliquid.xyz/evm"),
    (1101, "https://zkevm-rpc.com"),
    (1329, "https://evm-rpc.sei-apis.com"),
    (1868, "https://rpc.soneium.org"),
    (2741, "https://api.mainnet.abs.xyz"),
    (5000, "https://mantle-rpc.publicnode.com"),
    (8453, "https://base-rpc.publicnode.com"),
    (9745, "https://rpc.plasma.to"),
    (34443, "https://mainnet.mode.network"),
    (42161, "https://arbitrum-one-rpc.publicnode.com"),
    (43114, "https://avalanche-c-chain-rpc.publicnode.com"),
    (57073, "https://rpc-gel.inkonchain.com"),
    (59144, "https://linea-rpc.publicnode.com"),
    (80094, "https://rpc.berachain.com"),
    (81457, "https://rpc.blast.io"),
    (98866, "https://rpc.plume.org"),
    (534_352, "https://scroll-rpc.publicnode.com"),
    (747_474, "https://rpc.katana.network"),
];

/// The built-in endpoint for `chain_id`, if there is one.
pub fn default_endpoint(chain_id: u64) -> Option<&'static str> {
    DEFAULT_ENDPOINTS
        .iter()
        .find(|(id, _)| *id == chain_id)
        .map(|(_, url)| *url)
}

/// Chains that can be transferred on without an explicit endpoint.
pub fn chains_with_endpoints() -> impl Iterator<Item = u64> {
    DEFAULT_ENDPOINTS.iter().map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn known_chains_resolve_and_unknown_ones_do_not() {
        assert_eq!(
            default_endpoint(8453),
            Some("https://base-rpc.publicnode.com")
        );
        assert!(default_endpoint(89_999).is_none(), "Solana is not EVM");
        assert!(default_endpoint(0).is_none());
    }

    /// A duplicate id would silently shadow the second entry.
    #[test]
    fn every_chain_appears_once() {
        let ids: Vec<u64> = chains_with_endpoints().collect();
        let unique: BTreeSet<u64> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len());
    }

    /// A phrase-bearing process must not talk to an RPC in the clear: the
    /// endpoint sees the from-address of every transfer.
    #[test]
    fn endpoints_are_https() {
        for (id, url) in DEFAULT_ENDPOINTS {
            assert!(url.starts_with("https://"), "chain {id} is not https");
        }
    }
}
