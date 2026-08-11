//! Completes a wallet setup handshake without a browser, for development.
//!
//! Usage:
//!   `cargo run -p acp-connect --example fake_setup -- <url> new`
//!   `cargo run -p acp-connect --example fake_setup -- <url> import "<phrase>"`

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
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .expect("usage: fake_setup <url> new|import [phrase]");
    let mode = args.next().unwrap_or_else(|| "new".to_owned());
    let authority = url
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();

    let (_, page) = send(&authority, "GET", "/", None).await;
    let marker = r#"id="cfg">"#;
    let start = page.find(marker).expect("cfg") + marker.len();
    let end = start + page[start..].find("</script>").expect("cfg end");
    let cfg: serde_json::Value = serde_json::from_str(&page[start..end]).expect("cfg json");
    let token = cfg["token"].as_str().expect("token");

    if mode == "import" {
        let phrase = args.next().expect("import needs a phrase");
        let (status, body) = send(
            &authority,
            "POST",
            "/import",
            Some(serde_json::json!({ "token": token, "phrase": phrase })),
        )
        .await;
        assert_eq!(status, 200, "import failed: {body}");
        println!("imported");
        return;
    }

    let (status, body) = send(
        &authority,
        "POST",
        "/generate",
        Some(serde_json::json!({ "token": token, "words": 24 })),
    )
    .await;
    assert_eq!(status, 200, "generate failed: {body}");

    let v: serde_json::Value = serde_json::from_str(&body).expect("generate json");
    let words: Vec<&str> = v["words"]
        .as_array()
        .expect("words")
        .iter()
        .map(|w| w.as_str().expect("word"))
        .collect();
    let answers: Vec<serde_json::Value> = v["challenge"]
        .as_array()
        .expect("challenge")
        .iter()
        .map(|i| {
            let idx = usize::try_from(i.as_u64().expect("index")).expect("fits");
            serde_json::json!({ "index": idx, "word": words[idx] })
        })
        .collect();

    let (status, body) = send(
        &authority,
        "POST",
        "/confirm",
        Some(serde_json::json!({ "token": token, "answers": answers })),
    )
    .await;
    assert_eq!(status, 200, "confirm failed: {body}");
    println!("created");
}
