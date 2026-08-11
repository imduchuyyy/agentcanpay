//! Shows an existing recovery phrase to the user, in the browser.
//!
//! The phrase is handed in by the caller, which is the only part of the
//! system allowed to read the credential store. It reaches the browser only
//! when the user asks for it: until they press Show, the page has never seen
//! it, and pressing Hide removes it from the document rather than hiding it
//! with CSS.

use std::sync::{Arc, Mutex};

use askama::Template;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use crate::{
    APP_CSS, ConnectError, ConnectOptions, HTMX_JS, TOKEN_HEADER, no_store, serve_and_wait,
    session_token,
};

#[derive(Template)]
#[template(path = "reveal/page.html")]
struct PageTemplate<'a> {
    token: &'a str,
    address: &'a str,
    word_count: usize,
}

#[derive(Template)]
#[template(path = "reveal/hidden.html")]
struct HiddenTemplate {
    word_count: usize,
}

#[derive(Template)]
#[template(path = "reveal/shown.html")]
struct ShownTemplate {
    words: Vec<String>,
}

#[derive(Template)]
#[template(path = "reveal/closed.html")]
struct ClosedTemplate;

struct AppState {
    token: String,
    address: String,
    phrase: Zeroizing<String>,
    tx: Mutex<Option<oneshot::Sender<Result<(), ConnectError>>>>,
}

impl AppState {
    fn word_count(&self) -> usize {
        self.phrase.split(' ').count()
    }

    fn finish(&self, outcome: Result<(), ConnectError>) {
        if let Some(tx) = self.tx.lock().expect("state lock poisoned").take() {
            let _ = tx.send(outcome);
        }
    }

    fn authorized(&self, headers: &HeaderMap) -> bool {
        headers
            .get(TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|got| got == self.token)
    }
}

fn frag<T: Template>(t: &T) -> Response {
    match t.render() {
        Ok(body) => no_store(Html(body)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response(),
    }
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "bad session token").into_response()
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    frag(&PageTemplate {
        token: &state.token,
        address: &state.address,
        word_count: state.word_count(),
    })
}

async fn app_css() -> Response {
    ([(axum::http::header::CONTENT_TYPE, "text/css")], APP_CSS).into_response()
}

async fn htmx_js() -> Response {
    (
        [(axum::http::header::CONTENT_TYPE, "text/javascript")],
        HTMX_JS,
    )
        .into_response()
}

/// The only response that carries the phrase, and only on request.
async fn show_phrase(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }
    frag(&ShownTemplate {
        words: state.phrase.split(' ').map(str::to_owned).collect(),
    })
}

async fn hide(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }
    frag(&HiddenTemplate {
        word_count: state.word_count(),
    })
}

async fn done(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }
    state.finish(Ok(()));
    frag(&ClosedTemplate)
}

/// Serves the reveal page on loopback and returns once the user is done.
pub async fn run<F>(
    address: String,
    phrase: Zeroizing<String>,
    opts: ConnectOptions,
    on_ready: F,
) -> Result<(), ConnectError>
where
    F: FnOnce(&str),
{
    let (tx, rx) = oneshot::channel();
    let state = Arc::new(AppState {
        token: session_token(),
        address,
        phrase,
        tx: Mutex::new(Some(tx)),
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/app.css", get(app_css))
        .route("/htmx.js", get(htmx_js))
        .route("/phrase", get(show_phrase))
        .route("/hide", get(hide))
        .route("/done", post(done))
        .with_state(state);

    serve_and_wait(router, rx, &opts, on_ready).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hidden_screen_masks_every_word_and_shows_none() {
        let html = HiddenTemplate { word_count: 24 }.render().unwrap();
        assert_eq!(html.matches("••••••").count(), 24);
        assert!(html.contains("Show recovery phrase"));
    }

    #[test]
    fn the_shown_screen_lists_the_words_and_offers_copy() {
        let words: Vec<String> = (0..12).map(|i| format!("word{i}")).collect();
        let html = ShownTemplate {
            words: words.clone(),
        }
        .render()
        .unwrap();

        for w in &words {
            assert!(html.contains(w.as_str()), "missing {w}");
        }
        assert!(html.contains("data-copy"));
        assert!(html.contains(r#"hx-get="/hide""#));
    }

    /// The landing page must not carry the phrase: it arrives only from
    /// `/phrase`, after the user asks for it.
    #[test]
    fn the_landing_page_contains_no_words() {
        let html = PageTemplate {
            token: "deadbeef",
            address: "0xabc",
            word_count: 24,
        }
        .render()
        .unwrap();

        assert!(html.contains("X-Session-Token"));
        assert!(html.contains("0xabc"));
        assert_eq!(html.matches("••••••").count(), 24);
    }
}
