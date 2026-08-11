use std::collections::BTreeMap;

use acp_api::{Chain, Client, ListKind, Token};
use acp_keystore::Keystore;

use super::CommandError;
use crate::{cli::BalanceArgs, output::Output};

/// One token the wallet actually holds.
pub struct Holding {
    pub chain_id: u64,
    pub chain: String,
    pub symbol: String,
    pub address: String,
    pub amount: String,
    pub usd: f64,
    pub verified: bool,
}

/// Lists what the wallet holds, across every supported chain.
///
/// Reads metadata only — never the credential store — so this cannot
/// prompt. Balances come back with the token list, so there is no RPC
/// endpoint to configure.
pub async fn run(args: &BalanceArgs, out: &Output) -> Result<(), CommandError> {
    let meta = Keystore::open_default()?.load()?;
    let account = meta
        .account("evm")
        .ok_or_else(|| CommandError::NoAccountForChain("evm".into()))?;
    let address = account.address.parse().map_err(|_| {
        CommandError::NoAccountForChain(format!("unparseable address {}", account.address))
    })?;

    let client = Client::new()?;
    let chains = client.supported_chains().await?;
    let names: BTreeMap<u64, String> = chains
        .iter()
        .map(|c| (c.chain_id, c.name.clone()))
        .collect();

    let wanted = selected_chains(&chains, &args.chain)?;
    out.note(&format!(
        "Checking {} chain(s) for {}…",
        wanted.len(),
        account.address
    ));

    let lists = client
        .token_list(address, &wanted, ListKind::Trending)
        .await?;

    let holdings = collect(&lists, &names, args.min_usd);
    // Folded from 0.0 rather than summed: `Sum` for floats starts at -0.0
    // to preserve signed zeros, so an empty wallet would report "-0.00".
    let total = holdings.iter().fold(0.0, |acc, h| acc + h.usd);

    out.balances(&account.address, &holdings, total);
    Ok(())
}

/// Resolves `--chain` selectors against the supported list.
///
/// An unknown chain is an error rather than an empty result, so a typo does
/// not read as "you hold nothing".
fn selected_chains(chains: &[Chain], requested: &[String]) -> Result<Vec<u64>, CommandError> {
    if requested.is_empty() {
        return Ok(chains.iter().map(|c| c.chain_id).collect());
    }

    requested
        .iter()
        .map(|want| {
            chains
                .iter()
                .find(|c| c.chain_id.to_string() == *want || c.name.eq_ignore_ascii_case(want))
                .map(|c| c.chain_id)
                .ok_or_else(|| CommandError::UnknownChain(want.clone()))
        })
        .collect()
}

/// Keeps only what the wallet actually holds, richest first.
fn collect(
    lists: &BTreeMap<u64, Vec<Token>>,
    names: &BTreeMap<u64, String>,
    min_usd: f64,
) -> Vec<Holding> {
    let mut holdings: Vec<Holding> = lists
        .iter()
        .flat_map(|(chain_id, tokens)| {
            tokens
                .iter()
                .filter(|t| t.has_balance() && t.usd() >= min_usd)
                .map(move |t| Holding {
                    chain_id: *chain_id,
                    chain: names
                        .get(chain_id)
                        .cloned()
                        .unwrap_or_else(|| chain_id.to_string()),
                    symbol: t.symbol.clone(),
                    address: t.address.clone(),
                    amount: t.amount(),
                    usd: t.usd(),
                    verified: t.is_verified.unwrap_or(false),
                })
        })
        .collect();

    holdings.sort_by(|a, b| {
        b.usd
            .partial_cmp(&a.usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    holdings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(symbol: &str, balance: Option<&str>, usd: Option<f64>) -> Token {
        serde_json::from_value(serde_json::json!({
            "chainId": 1,
            "address": "0x0",
            "name": symbol,
            "symbol": symbol,
            "decimals": 18,
            "balance": balance,
            "balanceInUsd": usd,
        }))
        .unwrap()
    }

    fn names() -> BTreeMap<u64, String> {
        BTreeMap::from([(1, "Ethereum".to_owned())])
    }

    #[test]
    fn keeps_only_tokens_with_a_balance() {
        let lists = BTreeMap::from([(
            1,
            vec![
                token("ZERO", Some("0"), Some(0.0)),
                token("NONE", None, None),
                token("HELD", Some("1000000000000000000"), Some(5.0)),
            ],
        )]);

        let held = collect(&lists, &names(), 0.0);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].symbol, "HELD");
        assert_eq!(held[0].amount, "1");
        assert_eq!(held[0].chain, "Ethereum");
    }

    #[test]
    fn sorts_by_value_then_symbol() {
        let lists = BTreeMap::from([(
            1,
            vec![
                token("SMALL", Some("1000000000000000000"), Some(1.0)),
                token("BIG", Some("1000000000000000000"), Some(100.0)),
                token("BETA", Some("1000000000000000000"), Some(1.0)),
            ],
        )]);

        let held = collect(&lists, &names(), 0.0);
        let order: Vec<&str> = held.iter().map(|h| h.symbol.as_str()).collect();
        assert_eq!(order, ["BIG", "BETA", "SMALL"]);
    }

    /// A held token with no price must still be listed: unpriced is not
    /// the same as not owned.
    #[test]
    fn lists_held_tokens_that_have_no_price() {
        let lists =
            BTreeMap::from([(1, vec![token("NOPRICE", Some("5000000000000000000"), None)])]);

        let held = collect(&lists, &names(), 0.0);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].amount, "5");
        assert!((held[0].usd - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn min_usd_filters_dust() {
        let lists = BTreeMap::from([(
            1,
            vec![
                token("DUST", Some("1000000000000000000"), Some(0.001)),
                token("REAL", Some("1000000000000000000"), Some(50.0)),
            ],
        )]);

        let held = collect(&lists, &names(), 1.0);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].symbol, "REAL");
    }

    #[test]
    fn an_unnamed_chain_falls_back_to_its_id() {
        let lists = BTreeMap::from([(
            999,
            vec![token("X", Some("1000000000000000000"), Some(1.0))],
        )]);
        let held = collect(&lists, &names(), 0.0);
        assert_eq!(held[0].chain, "999");
    }

    fn chains() -> Vec<Chain> {
        serde_json::from_value(serde_json::json!([
            {"chainId": 1, "name": "Ethereum",
             "currency": {"address":"0xe","name":"Ether","symbol":"ETH","decimals":18}},
            {"chainId": 8453, "name": "Base",
             "currency": {"address":"0xe","name":"Ether","symbol":"ETH","decimals":18}}
        ]))
        .unwrap()
    }

    #[test]
    fn no_selector_means_every_supported_chain() {
        assert_eq!(selected_chains(&chains(), &[]).unwrap(), vec![1, 8453]);
    }

    #[test]
    fn chains_resolve_by_id_or_name_case_insensitively() {
        let picked = selected_chains(&chains(), &["8453".into(), "ethereum".into()]).unwrap();
        assert_eq!(picked, vec![8453, 1]);
    }

    /// An empty wallet must total 0.00, not -0.00.
    #[test]
    fn an_empty_wallet_totals_positive_zero() {
        let held: Vec<Holding> = Vec::new();
        let total = held.iter().fold(0.0, |acc, h| acc + h.usd);
        assert_eq!(format!("{total:.2}"), "0.00");
    }

    #[test]
    fn an_unknown_chain_is_an_error_not_an_empty_result() {
        let err = selected_chains(&chains(), &["Solana".into()]).unwrap_err();
        assert!(matches!(err, CommandError::UnknownChain(c) if c == "Solana"));
    }
}
