use alloy::{
    primitives::{Address, B256},
    sol,
    sol_types::{Eip712Domain, SolStruct, eip712_domain},
};

/// Human-readable string shown in the wallet's signing prompt.
pub const PURPOSE: &str = "Derive my agentcanpay wallet. Only sign this on a machine you control \
     — anyone with this signature controls the derived wallet.";

sol! {
    struct WalletSeed {
        string purpose;
        address account;
        uint32 index;
    }
}

/// `chainId` is pinned rather than read from the wallet.
///
/// If it tracked the wallet's selected network, a user switching chains
/// would change the digest and silently derive a different wallet.
pub fn domain() -> Eip712Domain {
    eip712_domain! {
        name: "agentcanpay",
        version: "1",
        chain_id: 1,
    }
}

pub fn digest(account: Address, index: u32) -> B256 {
    WalletSeed {
        purpose: PURPOSE.to_owned(),
        account,
        index,
    }
    .eip712_signing_hash(&domain())
}

/// The exact payload the page passes to `eth_signTypedData_v4`.
///
/// Built here rather than in JavaScript so the field order and types cannot
/// drift from the `sol!` struct above; a mismatch would produce a digest the
/// server refuses to verify.
pub fn typed_data_json(account: Address, index: u32) -> serde_json::Value {
    serde_json::json!({
        "types": {
            "EIP712Domain": [
                {"name": "name", "type": "string"},
                {"name": "version", "type": "string"},
                {"name": "chainId", "type": "uint256"},
            ],
            "WalletSeed": [
                {"name": "purpose", "type": "string"},
                {"name": "account", "type": "address"},
                {"name": "index", "type": "uint32"},
            ],
        },
        "primaryType": "WalletSeed",
        "domain": {"name": "agentcanpay", "version": "1", "chainId": 1},
        "message": {
            "purpose": PURPOSE,
            "account": account.to_string(),
            "index": index,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn account() -> Address {
        Address::from_str("0x00000000000000000000000000000000000000ff").unwrap()
    }

    #[test]
    fn digest_changes_with_account_and_index() {
        let other = Address::from_str("0x00000000000000000000000000000000000000ee").unwrap();
        assert_ne!(digest(account(), 0), digest(account(), 1));
        assert_ne!(digest(account(), 0), digest(other, 0));
    }

    #[test]
    fn digest_is_stable() {
        assert_eq!(digest(account(), 0), digest(account(), 0));
    }

    /// The JSON handed to the wallet must describe the same struct the
    /// server hashes, field for field.
    #[test]
    fn json_matches_the_sol_struct() {
        let json = typed_data_json(account(), 7);
        let fields = json["types"]["WalletSeed"].as_array().unwrap();
        let names: Vec<&str> = fields.iter().map(|f| f["name"].as_str().unwrap()).collect();
        let types: Vec<&str> = fields.iter().map(|f| f["type"].as_str().unwrap()).collect();

        assert_eq!(names, ["purpose", "account", "index"]);
        assert_eq!(types, ["string", "address", "uint32"]);
        assert_eq!(json["primaryType"], "WalletSeed");
        assert_eq!(json["message"]["index"], 7);
    }
}
