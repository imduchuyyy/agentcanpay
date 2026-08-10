//! Completes a connect handshake without a browser, for development.
//!
//! Signs with a throwaway key, so the wallet it creates is disposable.
//!
//! Usage: `cargo run -p acp-connect --example fake_browser -- <url>`

use acp_connect::typed_data;
use alloy::signers::{SignerSync, local::PrivateKeySigner};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

async fn send(
    authority: &str,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> (u16, String) {
    let stream = TcpStream::connect(authority).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);

    let payload = body.map_or_else(Bytes::new, |b| Bytes::from(b.to_string()));
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", authority)
        .header("content-type", "application/json")
        .body(Full::new(payload))
        .expect("request");

    let res = sender.send_request(req).await.expect("send");
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.expect("body").to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).expect("usage: fake_browser <url>");
    let authority = url
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();

    let (_, page) = send(&authority, "GET", "/", None).await;
    let start = page.find(r#"id="cfg">"#).expect("cfg") + r#"id="cfg">"#.len();
    let end = start + page[start..].find("</script>").expect("cfg end");
    let cfg: serde_json::Value = serde_json::from_str(&page[start..end]).expect("cfg json");
    let token = cfg["token"].as_str().expect("token");

    let signer = PrivateKeySigner::random();
    let address = signer.address();
    println!("signing as {address}");

    let (status, body) = send(
        &authority,
        "POST",
        "/prepare",
        Some(serde_json::json!({ "token": token, "address": address.to_string() })),
    )
    .await;
    assert_eq!(status, 200, "prepare failed: {body}");

    let typed: serde_json::Value = serde_json::from_str(&body).expect("typed data");
    let index = typed["typedData"]["message"]["index"]
        .as_u64()
        .expect("index");
    let digest = typed_data::digest(address, u32::try_from(index).expect("index fits"));
    let sig = signer.sign_hash_sync(&digest).expect("sign");

    let (status, body) = send(
        &authority,
        "POST",
        "/callback",
        Some(serde_json::json!({
            "token": token,
            "address": address.to_string(),
            "signature": sig.to_string(),
        })),
    )
    .await;
    assert_eq!(status, 200, "callback failed: {body}");
    println!("handshake complete");
}
