//! Derives a wallet from an external wallet's EIP-712 signature.
//!
//! Not currently reachable from the CLI; `setup` is what `create` runs.

use std::sync::{Arc, Mutex};

use alloy::primitives::{Address, Signature};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{
    ApiError, ConnectError, ConnectOptions, check_token, no_store, serve_and_wait, session_token,
    typed_data,
};

const PAGE: &str = include_str!("../assets/connect.html");

/// A verified browser signature.
pub struct Handshake {
    pub address: Address,
    pub signature: Signature,
}

/// Hand-written rather than derived: the signature is the wallet's root
/// secret, so it must never reach a log line or panic message.
impl std::fmt::Debug for Handshake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handshake")
            .field("address", &self.address)
            .field("signature", &"<redacted>")
            .finish()
    }
}

struct AppState {
    token: String,
    index: u32,
    tx: Mutex<Option<oneshot::Sender<Result<Handshake, ConnectError>>>>,
}

impl AppState {
    fn finish(&self, outcome: Result<Handshake, ConnectError>) {
        if let Some(tx) = self.tx.lock().expect("state lock poisoned").take() {
            let _ = tx.send(outcome);
        }
    }
}

#[derive(Deserialize)]
struct PrepareReq {
    token: String,
    address: String,
}

#[derive(Serialize)]
struct PrepareRes {
    #[serde(rename = "typedData")]
    typed_data: serde_json::Value,
}

#[derive(Deserialize)]
struct CallbackReq {
    token: String,
    address: String,
    signature: String,
}

#[derive(Deserialize)]
struct CancelReq {
    token: String,
    reason: String,
}

async fn index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = serde_json::json!({ "token": state.token });
    no_store(Html(PAGE.replace("__CFG__", &cfg.to_string())))
}

async fn prepare(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PrepareReq>,
) -> Result<Json<PrepareRes>, ApiError> {
    check_token(&state.token, &req.token)?;
    let address = parse_address(&req.address)?;
    Ok(Json(PrepareRes {
        typed_data: typed_data::typed_data_json(address, state.index),
    }))
}

async fn callback(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CallbackReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    check_token(&state.token, &req.token)?;
    let address = parse_address(&req.address)?;

    let signature = req
        .signature
        .parse::<Signature>()
        .map_err(|_| ApiError::bad_request("malformed signature"))?;

    // Never trust the address the page reports: recover it from the digest
    // the server built and require an exact match.
    let digest = typed_data::digest(address, state.index);
    let recovered = signature
        .recover_address_from_prehash(&digest)
        .map_err(|_| ApiError::bad_request("unrecoverable signature"))?;

    if recovered != address {
        state.finish(Err(ConnectError::AddressMismatch));
        return Err(ApiError::bad_request(
            "signature does not match the connected account; smart-contract \
             wallets cannot be used here",
        ));
    }

    state.finish(Ok(Handshake { address, signature }));
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn cancel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CancelReq>,
) -> Result<StatusCode, ApiError> {
    check_token(&state.token, &req.token)?;
    state.finish(Err(ConnectError::Cancelled(req.reason)));
    Ok(StatusCode::NO_CONTENT)
}

fn parse_address(s: &str) -> Result<Address, ApiError> {
    s.parse::<Address>()
        .map_err(|_| ApiError::bad_request("malformed address"))
}

/// Serves the connect page on loopback and waits for a verified signature.
pub async fn run<F>(
    index_at: u32,
    opts: ConnectOptions,
    on_ready: F,
) -> Result<Handshake, ConnectError>
where
    F: FnOnce(&str),
{
    let (tx, rx) = oneshot::channel();
    let state = Arc::new(AppState {
        token: session_token(),
        index: index_at,
        tx: Mutex::new(Some(tx)),
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/prepare", post(prepare))
        .route("/callback", post(callback))
        .route("/cancel", post(cancel))
        .with_state(state);

    serve_and_wait(router, rx, &opts, on_ready).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::signers::{SignerSync, local::PrivateKeySigner};

    fn state(
        index: u32,
    ) -> (
        Arc<AppState>,
        oneshot::Receiver<Result<Handshake, ConnectError>>,
    ) {
        let (tx, rx) = oneshot::channel();
        let s = Arc::new(AppState {
            token: "tok".into(),
            index,
            tx: Mutex::new(Some(tx)),
        });
        (s, rx)
    }

    #[tokio::test]
    async fn accepts_a_genuine_signature() {
        let signer = PrivateKeySigner::random();
        let address = signer.address();
        let sig = signer
            .sign_hash_sync(&typed_data::digest(address, 0))
            .unwrap();

        let (st, rx) = state(0);
        let accepted = callback(
            State(st),
            Json(CallbackReq {
                token: "tok".into(),
                address: address.to_string(),
                signature: sig.to_string(),
            }),
        )
        .await;
        assert!(accepted.is_ok(), "valid signature should be accepted");

        let got = rx.await.unwrap().unwrap();
        assert_eq!(got.address, address);
    }

    /// A signature over the right digest but from a different key must not
    /// be accepted as that account.
    #[tokio::test]
    async fn rejects_a_signature_from_another_key() {
        let victim = PrivateKeySigner::random().address();
        let attacker = PrivateKeySigner::random();
        let sig = attacker
            .sign_hash_sync(&typed_data::digest(victim, 0))
            .unwrap();

        let (st, rx) = state(0);
        let res = callback(
            State(st),
            Json(CallbackReq {
                token: "tok".into(),
                address: victim.to_string(),
                signature: sig.to_string(),
            }),
        )
        .await;

        assert!(res.is_err());
        assert!(matches!(
            rx.await.unwrap(),
            Err(ConnectError::AddressMismatch)
        ));
    }

    /// A signature valid for index 0 must not authorise index 1.
    #[tokio::test]
    async fn rejects_a_signature_for_a_different_index() {
        let signer = PrivateKeySigner::random();
        let address = signer.address();
        let sig = signer
            .sign_hash_sync(&typed_data::digest(address, 0))
            .unwrap();

        let (st, _rx) = state(1);
        let res = callback(
            State(st),
            Json(CallbackReq {
                token: "tok".into(),
                address: address.to_string(),
                signature: sig.to_string(),
            }),
        )
        .await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn rejects_a_bad_session_token() {
        let (st, _rx) = state(0);
        let res = prepare(
            State(st),
            Json(PrepareReq {
                token: "wrong".into(),
                address: "0x00000000000000000000000000000000000000ff".into(),
            }),
        )
        .await;

        assert!(res.is_err());
    }

    #[test]
    fn session_tokens_are_unique() {
        assert_ne!(session_token(), session_token());
        assert_eq!(session_token().len(), 64);
    }
}
