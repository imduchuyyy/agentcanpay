//! Drives the real setup server the way the browser page does.

use std::time::Duration;

use acp_connect::{
    ConnectOptions,
    setup::{SetupKind, SetupOutcome},
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpStream, sync::oneshot};

struct Res {
    status: StatusCode,
    body: String,
    cache_control: Option<String>,
}

async fn send(url: &str, method: &str, path: &str, body: Option<serde_json::Value>) -> Res {
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
    let cache_control = res
        .headers()
        .get("cache-control")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Res {
        status,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        cache_control,
    }
}

struct Session {
    url: String,
    token: String,
    page: String,
    result: tokio::task::JoinHandle<Result<SetupOutcome, acp_connect::ConnectError>>,
}

async fn start() -> Session {
    let (ready_tx, ready_rx) = oneshot::channel();

    let result = tokio::spawn(async move {
        acp_connect::setup::run(
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
    let res = send(&url, "GET", "/", None).await;
    assert_eq!(res.status, StatusCode::OK);

    let marker = r#"id="cfg">"#;
    let start = res.body.find(marker).expect("cfg block") + marker.len();
    let end = start + res.body[start..].find("</script>").expect("cfg close");
    let cfg: serde_json::Value = serde_json::from_str(&res.body[start..end]).expect("cfg is json");
    let token = cfg["token"].as_str().expect("token").to_owned();

    Session {
        url,
        token,
        page: res.body,
        result,
    }
}

#[tokio::test]
async fn generating_and_confirming_stores_the_shown_phrase() {
    let s = start().await;

    let gen_res = send(
        &s.url,
        "POST",
        "/generate",
        Some(serde_json::json!({ "token": s.token, "words": 24 })),
    )
    .await;
    assert_eq!(gen_res.status, StatusCode::OK);

    // A response carrying a recovery phrase must not be cacheable.
    assert_eq!(
        gen_res.cache_control.as_deref(),
        Some("no-store, max-age=0"),
        "phrase response must be no-store"
    );

    let v: serde_json::Value = serde_json::from_str(&gen_res.body).unwrap();
    let words: Vec<String> = v["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_owned())
        .collect();
    let challenge: Vec<usize> = v["challenge"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| usize::try_from(i.as_u64().unwrap()).unwrap())
        .collect();

    assert_eq!(words.len(), 24);
    assert_eq!(challenge.len(), 3);

    // The phrase must never be baked into the page. Checked against the
    // joined phrase rather than individual words, because several BIP-39
    // words ("cancel", "confirm") are also ordinary UI labels.
    let joined = words.join(" ");
    assert!(!s.page.contains(&joined), "initial page leaked the phrase");

    // And a page re-fetched while a phrase is pending must still not carry
    // it: only the /generate response may.
    let refetched = send(&s.url, "GET", "/", None).await;
    assert!(
        !refetched.body.contains(&joined),
        "reloaded page leaked the pending phrase"
    );

    let answers: Vec<serde_json::Value> = challenge
        .iter()
        .map(|i| serde_json::json!({ "index": i, "word": words[*i] }))
        .collect();
    let confirm = send(
        &s.url,
        "POST",
        "/confirm",
        Some(serde_json::json!({ "token": s.token, "answers": answers })),
    )
    .await;
    assert_eq!(confirm.status, StatusCode::OK, "confirm: {}", confirm.body);

    let outcome = s.result.await.unwrap().unwrap();
    assert_eq!(outcome.kind, SetupKind::Generated);
    assert_eq!(*outcome.phrase, words.join(" "));
}

#[tokio::test]
async fn a_wrong_confirmation_leaves_the_session_open_to_retry() {
    let s = start().await;

    let gen_res = send(
        &s.url,
        "POST",
        "/generate",
        Some(serde_json::json!({ "token": s.token, "words": 24 })),
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&gen_res.body).unwrap();
    let words: Vec<String> = v["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_owned())
        .collect();
    let challenge: Vec<usize> = v["challenge"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| usize::try_from(i.as_u64().unwrap()).unwrap())
        .collect();

    let wrong: Vec<serde_json::Value> = challenge
        .iter()
        .map(|i| serde_json::json!({ "index": i, "word": "wrong" }))
        .collect();
    let bad = send(
        &s.url,
        "POST",
        "/confirm",
        Some(serde_json::json!({ "token": s.token, "answers": wrong })),
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // Retrying with the right words must still work.
    let right: Vec<serde_json::Value> = challenge
        .iter()
        .map(|i| serde_json::json!({ "index": i, "word": words[*i] }))
        .collect();
    let ok = send(
        &s.url,
        "POST",
        "/confirm",
        Some(serde_json::json!({ "token": s.token, "answers": right })),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);

    assert_eq!(s.result.await.unwrap().unwrap().kind, SetupKind::Generated);
}

#[tokio::test]
async fn importing_a_phrase_completes_setup() {
    let s = start().await;
    let known = "test test test test test test test test test test test junk";

    let res = send(
        &s.url,
        "POST",
        "/import",
        Some(
            serde_json::json!({ "token": s.token, "phrase": format!("  TEST {}  ", &known[5..]) }),
        ),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK, "import: {}", res.body);

    let outcome = s.result.await.unwrap().unwrap();
    assert_eq!(outcome.kind, SetupKind::Imported);
    assert_eq!(*outcome.phrase, known);
}

#[tokio::test]
async fn an_invalid_import_is_reported_without_ending_the_session() {
    let s = start().await;

    let bad = send(
        &s.url,
        "POST",
        "/import",
        Some(serde_json::json!({ "token": s.token, "phrase": "abandon abandon abandon" })),
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert!(bad.body.contains("words"), "unhelpful error: {}", bad.body);

    let known = "test test test test test test test test test test test junk";
    let ok = send(
        &s.url,
        "POST",
        "/import",
        Some(serde_json::json!({ "token": s.token, "phrase": known })),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(s.result.await.unwrap().unwrap().kind, SetupKind::Imported);
}

#[tokio::test]
async fn another_local_process_cannot_drive_the_flow_without_the_token() {
    let s = start().await;

    for (path, body) in [
        (
            "/generate",
            serde_json::json!({ "token": "guess", "words": 24 }),
        ),
        (
            "/import",
            serde_json::json!({ "token": "guess", "phrase": "x" }),
        ),
    ] {
        let res = send(&s.url, "POST", path, Some(body)).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path} accepted a guess");
    }
}
