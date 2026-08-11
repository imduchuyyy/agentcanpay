//! Decodes recorded upstream responses.
//!
//! Fixtures rather than live calls: the test suite must not depend on a
//! third party being reachable. Re-record them with the commands in
//! `tests/fixtures/README.md` when the upstream shape changes.

use std::collections::BTreeMap;

use acp_api::{Chain, Token};
use serde::Deserialize;

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    result: T,
}

fn load<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(&path).expect("fixture");
    let envelope: Envelope<T> = serde_json::from_str(&raw).expect("envelope");
    assert!(envelope.success);
    envelope.result
}

#[test]
fn decodes_supported_chains() {
    let chains: Vec<Chain> = load("supported_chains.json");
    assert!(!chains.is_empty());

    let ethereum = chains
        .iter()
        .find(|c| c.chain_id == 1)
        .expect("Ethereum in fixture");
    assert_eq!(ethereum.name, "Ethereum");
    assert_eq!(ethereum.currency.symbol, "ETH");
    assert_eq!(ethereum.currency.decimals, 18);
    // Native currency is a pseudo-token at this sentinel address.
    assert_eq!(
        ethereum.currency.address.to_lowercase(),
        "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    );
}

#[test]
fn decodes_a_token_list_keyed_by_chain() {
    let lists: BTreeMap<String, Vec<Token>> = load("token_list.json");
    assert!(lists.contains_key("1"), "fixture should cover mainnet");

    let tokens = &lists["1"];
    assert!(!tokens.is_empty());
    assert!(tokens.iter().all(|t| t.chain_id == 1));
    assert!(tokens.iter().any(Token::has_balance));
}

/// Unranked and unpriced tokens come back with nulls, and a strict struct
/// would fail the whole listing over one of them.
#[test]
fn tolerates_null_optional_fields() {
    let raw = r#"{
      "chainId": 1,
      "address": "0x0000000000000000000000000000000000000000",
      "name": "Nulls",
      "symbol": "NUL",
      "decimals": 18,
      "logoURI": null,
      "isShortListed": false,
      "tags": [],
      "trendingRank": null,
      "marketCap": null,
      "totalVolume": null,
      "balance": null,
      "balanceInUsd": null,
      "isVerified": null
    }"#;

    let token: Token = serde_json::from_str(raw).expect("nulls should decode");
    assert!(!token.has_balance());
    assert_eq!(token.amount(), "0");
    assert!((token.usd() - 0.0).abs() < f64::EPSILON);
}

/// Unknown fields must not break decoding: upstream adds them without
/// warning, and every one would otherwise be an outage.
#[test]
fn ignores_fields_it_does_not_model() {
    let raw = r#"{
      "chainId": 8453,
      "address": "0x1",
      "name": "Future",
      "symbol": "FUT",
      "decimals": 6,
      "balance": "2500000",
      "somethingAddedNextQuarter": {"nested": true}
    }"#;

    let token: Token = serde_json::from_str(raw).expect("unknown fields should decode");
    assert_eq!(token.amount(), "2.5");
}
