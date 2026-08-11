//! Drives the real loopback server the way a browser would.

use std::time::Duration;

use acp_connect::{ConnectOptions, typed_data};
use alloy::{
    primitives::Address,
    signers::{SignerSync, local::PrivateKeySigner},
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpStream, sync::oneshot};

async fn send(
    url: &str,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, String) {
    let authority = url.trim_start_matches("http://").trim_end_matches('/');
    let stream = TcpStream::connect(authority).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);

    let payload = body.map_or_else(Bytes::new, |b| Bytes::from(b.to_string()));
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", authority)
        .header("content-type", "application/json")
        .body(Full::new(payload))
        .unwrap();

    let res = sender.send_request(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Pulls the session token out of the served page, which also checks that
/// config injection produced valid JSON.
fn token_from_page(page: &str) -> String {
    let start = page.find(r#"id="cfg">"#).expect("cfg block") + r#"id="cfg">"#.len();
    let end = start + page[start..].find("</script>").expect("cfg close");
    let cfg: serde_json::Value = serde_json::from_str(&page[start..end]).expect("cfg is json");
    cfg["token"].as_str().expect("token").to_owned()
}

struct Session {
    url: String,
    token: String,
    result: tokio::task::JoinHandle<
        Result<acp_connect::authorize::Handshake, acp_connect::ConnectError>,
    >,
}

async fn start(index: u32) -> Session {
    let (ready_tx, ready_rx) = oneshot::channel();

    let result = tokio::spawn(async move {
        acp_connect::authorize::run(
            index,
            ConnectOptions {
                timeout: Duration::from_secs(10),
                open_browser: false,
            },
            move |url| {
                let _ = ready_tx.send(url.to_owned());
            },
        )
        .await
    });

    let url = ready_rx.await.unwrap();
    let (status, page) = send(&url, "GET", "/", None).await;
    assert_eq!(status, StatusCode::OK);
    let token = token_from_page(&page);

    Session { url, token, result }
}

async fn prepare_and_sign(
    session: &Session,
    signer: &PrivateKeySigner,
    address: Address,
) -> StatusCode {
    let (status, body) = send(
        &session.url,
        "POST",
        "/prepare",
        Some(serde_json::json!({ "token": session.token, "address": address.to_string() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "prepare failed: {body}");

    // Sign what the server actually asked for, not a locally rebuilt copy.
    let typed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let index = typed["typedData"]["message"]["index"].as_u64().unwrap();
    let digest = typed_data::digest(address, u32::try_from(index).unwrap());
    let sig = signer.sign_hash_sync(&digest).unwrap();

    let (status, _) = send(
        &session.url,
        "POST",
        "/callback",
        Some(serde_json::json!({
            "token": session.token,
            "address": address.to_string(),
            "signature": sig.to_string(),
        })),
    )
    .await;
    status
}

#[tokio::test]
async fn completes_a_full_browser_handshake() {
    let signer = PrivateKeySigner::random();
    let address = signer.address();
    let session = start(0).await;

    assert_eq!(
        prepare_and_sign(&session, &signer, address).await,
        StatusCode::OK
    );

    let handshake = session.result.await.unwrap().unwrap();
    assert_eq!(handshake.address, address);
    assert_eq!(
        handshake
            .signature
            .recover_address_from_prehash(&typed_data::digest(address, 0))
            .unwrap(),
        address
    );
}

#[tokio::test]
async fn cancelling_in_the_browser_ends_the_wait() {
    let session = start(0).await;

    let (status, _) = send(
        &session.url,
        "POST",
        "/cancel",
        Some(serde_json::json!({ "token": session.token, "reason": "user rejected" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let err = session.result.await.unwrap().unwrap_err();
    assert!(matches!(err, acp_connect::ConnectError::Cancelled(_)));
}

#[tokio::test]
async fn a_stolen_page_without_the_token_gets_nowhere() {
    let session = start(0).await;

    let (status, _) = send(
        &session.url,
        "POST",
        "/prepare",
        Some(serde_json::json!({
            "token": "not-the-token",
            "address": Address::ZERO.to_string(),
        })),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}
