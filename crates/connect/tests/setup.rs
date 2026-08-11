//! Drives the real setup server the way htmx does: header-authenticated
//! requests that return HTML fragments.

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

/// `token` of `None` omits the session header entirely.
async fn send(url: &str, method: &str, path: &str, token: Option<&str>, form: Option<&str>) -> Res {
    let authority = url.trim_start_matches("http://").trim_end_matches('/');
    let stream = TcpStream::connect(authority).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);

    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", authority)
        .header("content-type", "application/x-www-form-urlencoded");
    if let Some(t) = token {
        req = req.header("x-session-token", t);
    }

    let payload = form.map_or_else(Bytes::new, |f| Bytes::from(f.to_owned()));
    let res = sender
        .send_request(req.body(Full::new(payload)).unwrap())
        .await
        .unwrap();

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

/// Reads the session token out of the `hx-headers` attribute on the root
/// element, which is where htmx itself picks it up.
fn token_from_page(page: &str) -> String {
    let key = "X-Session-Token";
    let after = &page[page.find(key).expect("hx-headers token") + key.len()..];
    let hex: String = after
        .chars()
        .skip_while(|c| !c.is_ascii_hexdigit())
        .take_while(char::is_ascii_hexdigit)
        .collect();
    assert_eq!(hex.len(), 64, "token: {hex}");
    hex
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
    let res = send(&url, "GET", "/", None, None).await;
    assert_eq!(res.status, StatusCode::OK);
    let token = token_from_page(&res.body);

    Session {
        url,
        token,
        page: res.body,
        result,
    }
}

/// Pulls the words out of the rendered phrase fragment.
fn words_from(html: &str) -> Vec<String> {
    html.split("<li>")
        .skip(1)
        .map(|chunk| chunk.split("</li>").next().unwrap().trim().to_owned())
        .collect()
}

/// Reads the challenge positions from the rendered input names.
fn positions_from(html: &str) -> Vec<usize> {
    html.split(r#"name="w"#)
        .skip(1)
        .map(|c| c.split('"').next().unwrap().parse().unwrap())
        .collect()
}

async fn generate(s: &Session, words: u32) -> Res {
    send(
        &s.url,
        "POST",
        &format!("/new?words={words}"),
        Some(&s.token),
        None,
    )
    .await
}

#[tokio::test]
async fn generating_and_confirming_stores_the_shown_phrase() {
    let s = start().await;

    let shown = generate(&s, 24).await;
    assert_eq!(shown.status, StatusCode::OK);
    assert_eq!(
        shown.cache_control.as_deref(),
        Some("no-store, max-age=0"),
        "phrase fragment must be no-store"
    );

    let words = words_from(&shown.body);
    assert_eq!(words.len(), 24);

    // The phrase must never be baked into the page. Checked against the
    // joined phrase rather than individual words, because several BIP-39
    // words ("cancel", "confirm") are also ordinary UI labels.
    let joined = words.join(" ");
    assert!(!s.page.contains(&joined), "initial page leaked the phrase");

    let verify = send(&s.url, "GET", "/verify", Some(&s.token), None).await;
    assert_eq!(verify.status, StatusCode::OK);
    let positions = positions_from(&verify.body);
    assert_eq!(positions.len(), 3);

    let form = positions
        .iter()
        .map(|i| format!("w{i}={}", words[*i]))
        .collect::<Vec<_>>()
        .join("&");
    let done = send(&s.url, "POST", "/confirm", Some(&s.token), Some(&form)).await;
    assert_eq!(done.status, StatusCode::OK, "confirm: {}", done.body);

    let outcome = s.result.await.unwrap().unwrap();
    assert_eq!(outcome.kind, SetupKind::Generated);
    assert_eq!(*outcome.phrase, joined);
}

/// Re-rendering the phrase must show the same words, not mint new ones —
/// otherwise the user's written backup would silently stop matching.
#[tokio::test]
async fn showing_the_phrase_again_does_not_regenerate_it() {
    let s = start().await;

    let first = words_from(&generate(&s, 24).await.body);
    let again = send(&s.url, "GET", "/phrase", Some(&s.token), None).await;
    assert_eq!(again.status, StatusCode::OK);

    assert_eq!(words_from(&again.body), first);
}

#[tokio::test]
async fn a_wrong_confirmation_re_renders_the_form_and_allows_retry() {
    let s = start().await;
    let words = words_from(&generate(&s, 24).await.body);
    let positions = positions_from(
        &send(&s.url, "GET", "/verify", Some(&s.token), None)
            .await
            .body,
    );

    let wrong = positions
        .iter()
        .map(|i| format!("w{i}=wrong"))
        .collect::<Vec<_>>()
        .join("&");
    let bad = send(&s.url, "POST", "/confirm", Some(&s.token), Some(&wrong)).await;

    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    // htmx swaps 4xx bodies, so the response must be the form again with a
    // message, not a bare error.
    assert!(bad.body.contains("do not match"), "body: {}", bad.body);
    assert_eq!(
        positions_from(&bad.body),
        positions,
        "form was not re-rendered"
    );

    let right = positions
        .iter()
        .map(|i| format!("w{i}={}", words[*i]))
        .collect::<Vec<_>>()
        .join("&");
    let ok = send(&s.url, "POST", "/confirm", Some(&s.token), Some(&right)).await;
    assert_eq!(ok.status, StatusCode::OK);

    assert_eq!(s.result.await.unwrap().unwrap().kind, SetupKind::Generated);
}

/// Answering only some positions must not pass by vacuous truth.
#[tokio::test]
async fn a_partial_answer_set_is_rejected() {
    let s = start().await;
    let words = words_from(&generate(&s, 24).await.body);
    let positions = positions_from(
        &send(&s.url, "GET", "/verify", Some(&s.token), None)
            .await
            .body,
    );

    let one = format!("w{}={}", positions[0], words[positions[0]]);
    let res = send(&s.url, "POST", "/confirm", Some(&s.token), Some(&one)).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn importing_a_phrase_completes_setup() {
    let s = start().await;
    let known = "test test test test test test test test test test test junk";

    let res = send(
        &s.url,
        "POST",
        "/import",
        Some(&s.token),
        Some(&format!("phrase=  TEST {}  ", &known[5..])),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK, "import: {}", res.body);

    let outcome = s.result.await.unwrap().unwrap();
    assert_eq!(outcome.kind, SetupKind::Imported);
    assert_eq!(*outcome.phrase, known);
}

#[tokio::test]
async fn an_invalid_import_re_renders_the_form_and_allows_retry() {
    let s = start().await;

    let bad = send(
        &s.url,
        "POST",
        "/import",
        Some(&s.token),
        Some("phrase=abandon abandon abandon"),
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert!(bad.body.contains("words"), "unhelpful error: {}", bad.body);
    assert!(bad.body.contains("<textarea"), "form was not re-rendered");

    let known = "test test test test test test test test test test test junk";
    let ok = send(
        &s.url,
        "POST",
        "/import",
        Some(&s.token),
        Some(&format!("phrase={known}")),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(s.result.await.unwrap().unwrap().kind, SetupKind::Imported);
}

#[tokio::test]
async fn another_local_process_cannot_drive_the_flow_without_the_token() {
    let s = start().await;

    for (method, path, form) in [
        ("POST", "/new?words=24", None),
        ("GET", "/phrase", None),
        ("GET", "/verify", None),
        ("POST", "/import", Some("phrase=x")),
        ("POST", "/confirm", Some("w0=x")),
        ("POST", "/cancel", None),
    ] {
        let guessed = send(&s.url, method, path, Some("not-the-token"), form).await;
        assert_eq!(guessed.status, StatusCode::FORBIDDEN, "{path} took a guess");

        let omitted = send(&s.url, method, path, None, form).await;
        assert_eq!(
            omitted.status,
            StatusCode::FORBIDDEN,
            "{path} allowed a missing token"
        );
    }
}

/// The page is useless without its assets, and fetching them from a CDN
/// would put a third party in front of a recovery phrase.
#[tokio::test]
async fn assets_are_served_from_the_binary() {
    let s = start().await;

    let css = send(&s.url, "GET", "/app.css", None, None).await;
    assert_eq!(css.status, StatusCode::OK);
    assert!(css.body.contains("ol.words"));

    let js = send(&s.url, "GET", "/htmx.js", None, None).await;
    assert_eq!(js.status, StatusCode::OK);
    assert!(js.body.contains("htmx"), "htmx.js did not look like htmx");

    assert!(
        !s.page.contains("//unpkg.com") && !s.page.contains("//cdn."),
        "page referenced an external origin"
    );
}
