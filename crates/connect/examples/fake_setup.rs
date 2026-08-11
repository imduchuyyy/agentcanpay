//! Completes a wallet setup without a browser, for development.
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
    token: Option<&str>,
    form: Option<String>,
) -> (u16, String) {
    let stream = TcpStream::connect(authority).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);

    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", authority)
        .header("content-type", "application/x-www-form-urlencoded");
    if let Some(t) = token {
        req = req.header("x-session-token", t);
    }

    let payload = form.map_or_else(Bytes::new, Bytes::from);
    let res = sender
        .send_request(req.body(Full::new(payload)).expect("request"))
        .await
        .expect("send");
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.expect("body").to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
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

fn words_from(html: &str) -> Vec<String> {
    html.split("<li>")
        .skip(1)
        .map(|c| c.split("</li>").next().unwrap().trim().to_owned())
        .collect()
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

    let (_, page) = send(&authority, "GET", "/", None, None).await;
    let token = token_from_page(&page);

    if mode == "import" {
        let phrase = args.next().expect("import needs a phrase");
        let (status, body) = send(
            &authority,
            "POST",
            "/import",
            Some(&token),
            Some(format!("phrase={phrase}")),
        )
        .await;
        assert_eq!(status, 200, "import failed: {body}");
        println!("imported");
        return;
    }

    let (status, body) = send(&authority, "POST", "/new?words=24", Some(&token), None).await;
    assert_eq!(status, 200, "generate failed: {body}");
    println!("phrase has {} words", words_from(&body).len());

    let (status, body) = send(&authority, "POST", "/save", Some(&token), None).await;
    assert_eq!(status, 200, "save failed: {body}");
    println!("created");
}
