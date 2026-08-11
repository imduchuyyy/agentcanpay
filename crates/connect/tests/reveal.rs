//! Drives the real reveal server the way htmx does.

use std::time::Duration;

use acp_connect::ConnectOptions;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpStream, sync::oneshot};
use zeroize::Zeroizing;

const PHRASE: &str = "test test test test test test test test test test test junk";

struct Res {
    status: StatusCode,
    body: String,
    cache_control: Option<String>,
}

async fn send(url: &str, method: &str, path: &str, token: Option<&str>) -> Res {
    let authority = url.trim_start_matches("http://").trim_end_matches('/');
    let stream = TcpStream::connect(authority).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);

    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", authority);
    if let Some(t) = token {
        req = req.header("x-session-token", t);
    }

    let res = sender
        .send_request(req.body(Full::new(Bytes::new())).unwrap())
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
    result: tokio::task::JoinHandle<Result<(), acp_connect::ConnectError>>,
}

fn token_from_page(page: &str) -> String {
    let key = "X-Session-Token";
    let after = &page[page.find(key).expect("hx-headers token") + key.len()..];
    after
        .chars()
        .skip_while(|c| !c.is_ascii_hexdigit())
        .take_while(char::is_ascii_hexdigit)
        .collect()
}

async fn start() -> Session {
    let (ready_tx, ready_rx) = oneshot::channel();

    let result = tokio::spawn(async move {
        acp_connect::reveal::run(
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned(),
            Zeroizing::new(PHRASE.to_owned()),
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

    Session {
        token: token_from_page(&res.body),
        page: res.body,
        url,
        result,
    }
}

/// The landing page must not carry the phrase. A user who opens the page and
/// walks away has not exposed anything.
#[tokio::test]
async fn the_phrase_is_absent_until_it_is_asked_for() {
    let s = start().await;

    assert!(!s.page.contains(PHRASE), "landing page leaked the phrase");
    assert!(!s.page.contains("junk"), "landing page leaked a word");
    assert!(s.page.contains("Show recovery phrase"));
    // The wallet being revealed is named, so the user knows which it is.
    assert!(
        s.page
            .contains("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")
    );

    let shown = send(&s.url, "GET", "/phrase", Some(&s.token)).await;
    assert_eq!(shown.status, StatusCode::OK);
    assert_eq!(
        shown.cache_control.as_deref(),
        Some("no-store, max-age=0"),
        "phrase fragment must be no-store"
    );
    for word in PHRASE.split(' ') {
        assert!(shown.body.contains(word), "missing {word}");
    }
}

/// Hiding must remove the words from the document, not merely style them
/// out of view: a page left open should not still contain them.
#[tokio::test]
async fn hiding_removes_the_words_from_the_document() {
    let s = start().await;

    let shown = send(&s.url, "GET", "/phrase", Some(&s.token)).await;
    assert!(shown.body.contains("junk"));

    let hidden = send(&s.url, "GET", "/hide", Some(&s.token)).await;
    assert_eq!(hidden.status, StatusCode::OK);
    assert!(!hidden.body.contains("junk"), "hidden fragment kept a word");
    assert_eq!(hidden.body.matches("••••••").count(), 12);
}

#[tokio::test]
async fn done_ends_the_wait() {
    let s = start().await;

    let done = send(&s.url, "POST", "/done", Some(&s.token)).await;
    assert_eq!(done.status, StatusCode::OK);
    assert!(!done.body.contains("junk"));

    s.result.await.unwrap().unwrap();
}

#[tokio::test]
async fn another_local_process_cannot_read_the_phrase() {
    let s = start().await;

    for (method, path) in [("GET", "/phrase"), ("GET", "/hide"), ("POST", "/done")] {
        let guessed = send(&s.url, method, path, Some("not-the-token")).await;
        assert_eq!(guessed.status, StatusCode::FORBIDDEN, "{path} took a guess");
        assert!(!guessed.body.contains("junk"), "{path} leaked a word");

        let omitted = send(&s.url, method, path, None).await;
        assert_eq!(
            omitted.status,
            StatusCode::FORBIDDEN,
            "{path} allowed a missing token"
        );
        assert!(!omitted.body.contains("junk"), "{path} leaked a word");
    }
}
