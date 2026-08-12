//! Client for the Socket.tech HTTP API.
//!
//! Named for what it is — the project's outbound API client — rather than
//! for the vendor, since swap and bridge endpoints land here too.
//!
//! Only the endpoints needed so far are modelled. The transport, the
//! response envelope and the error mapping are shared, so adding swap or
//! bridge endpoints later means adding a module, not another client.

pub mod chains;
pub mod error;
pub mod tokens;

pub use chains::{Chain, Currency};
pub use error::ApiError;
pub use tokens::{ListKind, NATIVE_TOKEN_ADDRESS, Token};

use std::{collections::BTreeMap, time::Duration};

use alloy::primitives::Address;
use serde::{Deserialize, de::DeserializeOwned};

pub const DEFAULT_BASE_URL: &str = "https://public-backend.socket.tech";

/// Listing every supported chain takes seconds and returns about a
/// megabyte, so the default has to be generous.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Every response is wrapped in this shape, including failures.
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    success: bool,
    // A path-based default rather than a bare one: `#[serde(default)]` on a
    // generic field makes the derive demand `T: Default`.
    #[serde(default = "Option::default")]
    result: Option<T>,
    #[serde(default)]
    message: Option<serde_json::Value>,
}

pub struct Client {
    base_url: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new() -> Result<Self, ApiError> {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(concat!("agentcanpay/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ApiError::Transport(e.to_string()))?;

        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            http,
        })
    }

    /// Issues a GET and unwraps the envelope.
    ///
    /// A 200 carrying `success: false` is still a failure, so the envelope
    /// is checked as well as the status.
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .query(query)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::Status {
                status: status.as_u16(),
            });
        }

        let body = response.text().await?;
        let envelope: Envelope<T> =
            serde_json::from_str(&body).map_err(|e| ApiError::Decode(e.to_string()))?;

        match (envelope.success, envelope.result) {
            (true, Some(result)) => Ok(result),
            _ => Err(ApiError::Upstream(
                envelope
                    .message
                    .map_or_else(|| "no message".to_owned(), |m| m.to_string()),
            )),
        }
    }

    /// Every chain Socket can route through.
    pub async fn supported_chains(&self) -> Result<Vec<Chain>, ApiError> {
        self.get("/v3/swap/supported-chains", &[]).await
    }

    /// Tokens on `chain_ids`, each carrying `user`'s holding of it.
    ///
    /// Balances come back with the list, so no RPC endpoint or on-chain
    /// call is needed to read a wallet.
    pub async fn token_list(
        &self,
        user: Address,
        chain_ids: &[u64],
        list: ListKind,
    ) -> Result<BTreeMap<u64, Vec<Token>>, ApiError> {
        let ids = chain_ids
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");

        // Keyed by chain id as a string upstream; re-keyed to numbers here
        // so callers can join against `Chain::chain_id`.
        let raw: BTreeMap<String, Vec<Token>> = self
            .get(
                "/v3/swap/tokens/list",
                &[
                    ("userAddress", user.to_string()),
                    ("chainIds", ids),
                    ("list", list.as_str().to_owned()),
                ],
            )
            .await?;

        raw.into_iter()
            .map(|(k, v)| {
                k.parse::<u64>()
                    .map(|id| (id, v))
                    .map_err(|_| ApiError::Decode(format!("chain id `{k}` is not a number")))
            })
            .collect()
    }
}
