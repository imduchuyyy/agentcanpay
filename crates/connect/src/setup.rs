//! Wallet setup in the browser: generate a new phrase or import one.
//!
//! The phrase exists in three places and no others: this process's memory,
//! the loopback response that renders it for the user, and the credential
//! store. It is never returned through the CLI's stdout or stderr, because
//! the caller there is an agent whose output is read and logged.
//!
//! Screens are server-rendered fragments swapped in by htmx. There is no
//! client-side model of the flow, so the browser cannot hold a view of the
//! wallet that the server disagrees with.

use std::sync::{Arc, Mutex};

use acp_wallet::{WordCount, generate_phrase, normalize_phrase, validate_phrase};
use askama::Template;
use axum::{
    Form, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use crate::{ConnectError, ConnectOptions, no_store, serve_and_wait, session_token};

const HTMX_JS: &str = include_str!("../assets/htmx.min.js");
const APP_CSS: &str = include_str!("../assets/app.css");

const TOKEN_HEADER: &str = "x-session-token";

/// How many words the user is asked to re-enter to prove they wrote the
/// phrase down. Enough to defeat idle clicking, few enough to be bearable.
const CHALLENGE_WORDS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupKind {
    Generated,
    Imported,
}

/// The result of a completed setup.
pub struct SetupOutcome {
    pub phrase: Zeroizing<String>,
    pub kind: SetupKind,
}

impl std::fmt::Debug for SetupOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupOutcome")
            .field("kind", &self.kind)
            .field("phrase", &"<redacted>")
            .finish()
    }
}

#[derive(Template)]
#[template(path = "setup/page.html")]
struct PageTemplate<'a> {
    token: &'a str,
}

#[derive(Template)]
#[template(path = "setup/choose.html")]
struct ChooseTemplate;

#[derive(Template)]
#[template(path = "setup/length.html")]
struct LengthTemplate;

#[derive(Template)]
#[template(path = "setup/phrase.html")]
struct PhraseTemplate {
    words: Vec<String>,
}

#[derive(Template)]
#[template(path = "setup/verify.html")]
struct VerifyTemplate {
    positions: Vec<usize>,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "setup/import.html")]
struct ImportTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "setup/done.html")]
struct DoneTemplate;

/// A phrase that has been shown but not yet confirmed.
///
/// The challenge positions live here rather than in the page, so the user
/// cannot pick which words they are asked for.
struct Pending {
    phrase: Zeroizing<String>,
    challenge: Vec<usize>,
}

struct AppState {
    token: String,
    pending: Mutex<Option<Pending>>,
    tx: Mutex<Option<oneshot::Sender<Result<SetupOutcome, ConnectError>>>>,
}

impl AppState {
    fn finish(&self, outcome: Result<SetupOutcome, ConnectError>) {
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

/// Renders a fragment, or a 500 if a template is broken.
fn frag<T: Template>(t: &T) -> Response {
    match t.render() {
        Ok(body) => no_store(Html(body)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response(),
    }
}

/// A fragment returned with a non-2xx status.
///
/// htmx is configured to swap 4xx bodies, so a validation failure re-renders
/// the same screen with its message rather than dead-ending, while the
/// status code still says the request failed.
fn frag_status<T: Template>(status: StatusCode, t: &T) -> Response {
    match t.render() {
        Ok(body) => (status, no_store(Html(body))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response(),
    }
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "bad session token").into_response()
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    frag(&PageTemplate {
        token: &state.token,
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

async fn choose(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }
    frag(&ChooseTemplate)
}

async fn length(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }
    frag(&LengthTemplate)
}

/// Phrase length is chosen here rather than on the command line: the agent
/// invoking the CLI has no basis for the choice, and it is the user's.
#[derive(Deserialize)]
struct NewQuery {
    words: usize,
}

/// Generates a phrase and renders it for display.
///
/// Regenerating on each call is deliberate: if the user backs up and starts
/// again, the phrase they are looking at is always the one that will be
/// saved.
async fn new_wallet(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<NewQuery>,
) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }

    let count = match q.words {
        12 => WordCount::Twelve,
        24 => WordCount::TwentyFour,
        _ => return (StatusCode::BAD_REQUEST, "unsupported length").into_response(),
    };

    let Ok(phrase) = generate_phrase(count) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not generate").into_response();
    };

    let words: Vec<String> = phrase.split(' ').map(str::to_owned).collect();
    let challenge = challenge_indices(words.len(), CHALLENGE_WORDS);
    *state.pending.lock().expect("state lock poisoned") = Some(Pending { phrase, challenge });

    frag(&PhraseTemplate { words })
}

/// Re-renders the pending phrase without generating a new one.
async fn phrase(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }

    let guard = state.pending.lock().expect("state lock poisoned");
    let Some(pending) = guard.as_ref() else {
        return (StatusCode::BAD_REQUEST, "nothing generated yet").into_response();
    };

    frag(&PhraseTemplate {
        words: pending.phrase.split(' ').map(str::to_owned).collect(),
    })
}

async fn verify(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }

    let guard = state.pending.lock().expect("state lock poisoned");
    let Some(pending) = guard.as_ref() else {
        return (StatusCode::BAD_REQUEST, "nothing generated yet").into_response();
    };

    frag(&VerifyTemplate {
        positions: pending.challenge.clone(),
        error: None,
    })
}

async fn confirm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }

    let guard = state.pending.lock().expect("state lock poisoned");
    let Some(pending) = guard.as_ref() else {
        return (StatusCode::BAD_REQUEST, "nothing generated yet").into_response();
    };

    let words: Vec<&str> = pending.phrase.split(' ').collect();
    let all_correct = pending.challenge.iter().all(|i| {
        form.get(&format!("w{i}"))
            .map(|answer| answer.trim().to_lowercase())
            .is_some_and(|answer| words.get(*i).is_some_and(|w| *w == answer))
    });

    // A wrong answer is a retry, not a failure: the session stays open so
    // the user can look at their notes again.
    if !all_correct {
        return frag_status(
            StatusCode::BAD_REQUEST,
            &VerifyTemplate {
                positions: pending.challenge.clone(),
                error: Some("Those words do not match. Check your backup and try again.".into()),
            },
        );
    }

    let phrase = pending.phrase.clone();
    drop(guard);

    state.finish(Ok(SetupOutcome {
        phrase,
        kind: SetupKind::Generated,
    }));
    frag(&DoneTemplate)
}

async fn import_form(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }
    frag(&ImportTemplate { error: None })
}

#[derive(Deserialize)]
struct ImportForm {
    phrase: String,
}

async fn import(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(form): Form<ImportForm>,
) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }

    let phrase = normalize_phrase(&form.phrase);
    if let Err(e) = validate_phrase(&phrase) {
        return frag_status(
            StatusCode::BAD_REQUEST,
            &ImportTemplate {
                error: Some(e.to_string()),
            },
        );
    }

    state.finish(Ok(SetupOutcome {
        phrase,
        kind: SetupKind::Imported,
    }));
    frag(&DoneTemplate)
}

async fn cancel(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return forbidden();
    }
    state.finish(Err(ConnectError::Cancelled(
        "cancelled in the browser".into(),
    )));
    frag(&ChooseTemplate)
}

/// Picks `n` distinct positions in `0..len`.
fn challenge_indices(len: usize, n: usize) -> Vec<usize> {
    let mut picked: Vec<usize> = Vec::with_capacity(n);
    while picked.len() < n.min(len) {
        let i = random_below(len);
        if !picked.contains(&i) {
            picked.push(i);
        }
    }
    picked.sort_unstable();
    picked
}

fn random_below(n: usize) -> usize {
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).expect("system entropy source unavailable");
    usize::try_from(u64::from_le_bytes(buf) % n as u64).expect("modulo keeps this in range")
}

/// Serves the setup page on loopback and waits for the user to finish.
///
/// Takes no choice of its own: whether this becomes a new wallet or an
/// imported one, and how long the phrase is, are all decided in the page.
pub async fn run<F>(opts: ConnectOptions, on_ready: F) -> Result<SetupOutcome, ConnectError>
where
    F: FnOnce(&str),
{
    let (tx, rx) = oneshot::channel();
    let state = Arc::new(AppState {
        token: session_token(),
        pending: Mutex::new(None),
        tx: Mutex::new(Some(tx)),
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/app.css", get(app_css))
        .route("/htmx.js", get(htmx_js))
        .route("/choose", get(choose))
        .route("/length", get(length))
        .route("/new", post(new_wallet))
        .route("/phrase", get(phrase))
        .route("/verify", get(verify))
        .route("/confirm", post(confirm))
        .route("/import", get(import_form).post(import))
        .route("/cancel", post(cancel))
        .with_state(state);

    serve_and_wait(router, rx, &opts, on_ready).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_picks_distinct_in_range_positions() {
        for _ in 0..50 {
            let idx = challenge_indices(24, CHALLENGE_WORDS);
            assert_eq!(idx.len(), CHALLENGE_WORDS);
            assert!(idx.iter().all(|i| *i < 24));
            let mut sorted = idx.clone();
            sorted.dedup();
            assert_eq!(sorted.len(), CHALLENGE_WORDS, "indices must be distinct");
        }
    }

    /// Templates are compile-time checked, but rendering is still worth a
    /// smoke test: a missing loop variable is a compile error, an escaping
    /// mistake is not.
    #[test]
    fn phrase_template_escapes_and_lists_every_word() {
        let words: Vec<String> = (0..24).map(|i| format!("word{i}")).collect();
        let html = PhraseTemplate {
            words: words.clone(),
        }
        .render()
        .unwrap();

        for w in &words {
            assert!(html.contains(w.as_str()), "missing {w}");
        }
        assert_eq!(html.matches("<li>").count(), 24);
    }

    #[test]
    fn error_messages_are_html_escaped() {
        let html = ImportTemplate {
            error: Some("<img src=x onerror=alert(1)>".into()),
        }
        .render()
        .unwrap();

        // Askama escapes to numeric entities rather than named ones.
        assert!(!html.contains("<img"), "error message was not escaped");
        assert!(html.contains("&#60;img"), "unexpected escaping: {html}");
    }

    #[test]
    fn verify_template_labels_positions_from_one() {
        let html = VerifyTemplate {
            positions: vec![0, 5, 11],
            error: None,
        }
        .render()
        .unwrap();

        assert!(html.contains("Word #1"));
        assert!(html.contains("Word #6"));
        assert!(html.contains("Word #12"));
        assert!(html.contains(r#"name="w0""#));
        assert!(html.contains(r#"name="w11""#));
    }

    #[test]
    fn the_page_carries_the_token_for_htmx_to_send() {
        let html = PageTemplate { token: "deadbeef" }.render().unwrap();
        assert!(html.contains("X-Session-Token"));
        assert!(html.contains("deadbeef"));
    }
}
