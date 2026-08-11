//! Wallet setup in the browser: generate a new phrase or import one.
//!
//! The phrase exists in three places and no others: this process's memory,
//! the loopback response that renders it for the user, and the credential
//! store. It is never returned through the CLI's stdout or stderr, because
//! the caller there is an agent whose output is read and logged.

use std::sync::{Arc, Mutex};

use acp_wallet::{WordCount, generate_phrase, normalize_phrase, validate_phrase};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use crate::{
    ApiError, ConnectError, ConnectOptions, check_token, no_store, serve_and_wait, session_token,
};

const PAGE: &str = include_str!("../assets/setup.html");

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

struct AppState {
    token: String,
    /// Generated but not yet confirmed. Held here rather than in the page
    /// so a reload cannot silently swap in a phrase the user never saw.
    pending: Mutex<Option<Zeroizing<String>>>,
    tx: Mutex<Option<oneshot::Sender<Result<SetupOutcome, ConnectError>>>>,
}

impl AppState {
    fn finish(&self, outcome: Result<SetupOutcome, ConnectError>) {
        if let Some(tx) = self.tx.lock().expect("state lock poisoned").take() {
            let _ = tx.send(outcome);
        }
    }
}

/// Phrase length is chosen here rather than on the command line: the agent
/// invoking the CLI has no basis for the choice, and it is the user's.
#[derive(Deserialize)]
struct GenerateReq {
    token: String,
    words: usize,
}

#[derive(Serialize)]
struct GenerateRes {
    words: Vec<String>,
    challenge: Vec<usize>,
}

#[derive(Deserialize)]
struct ConfirmReq {
    token: String,
    answers: Vec<ConfirmAnswer>,
}

#[derive(Deserialize)]
struct ConfirmAnswer {
    index: usize,
    word: String,
}

#[derive(Deserialize)]
struct ImportReq {
    token: String,
    phrase: String,
}

#[derive(Deserialize)]
struct CancelReq {
    token: String,
    reason: String,
}

#[derive(Serialize)]
struct AddressRes {
    ok: bool,
}

async fn index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = serde_json::json!({ "token": state.token });
    no_store(Html(PAGE.replace("__CFG__", &cfg.to_string())))
}

/// Generates a phrase and returns it for display.
///
/// Regenerating on each call is deliberate: if the user reloads mid-flow,
/// the phrase they are looking at is always the one that will be saved.
async fn generate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GenerateReq>,
) -> Result<impl IntoResponse, ApiError> {
    check_token(&state.token, &req.token)?;

    let words = match req.words {
        12 => WordCount::Twelve,
        24 => WordCount::TwentyFour,
        n => return Err(ApiError::bad_request(format!("unsupported length {n}"))),
    };

    let phrase = generate_phrase(words)
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "could not generate"))?;
    let words: Vec<String> = phrase.split(' ').map(str::to_owned).collect();
    let challenge = challenge_indices(words.len(), CHALLENGE_WORDS);

    *state.pending.lock().expect("state lock poisoned") = Some(phrase);

    Ok(no_store(Json(GenerateRes { words, challenge })))
}

async fn confirm(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfirmReq>,
) -> Result<Json<AddressRes>, ApiError> {
    check_token(&state.token, &req.token)?;

    let phrase = state
        .pending
        .lock()
        .expect("state lock poisoned")
        .clone()
        .ok_or_else(|| ApiError::bad_request("no phrase has been generated yet"))?;

    let words: Vec<&str> = phrase.split(' ').collect();
    let all_correct = req.answers.len() == CHALLENGE_WORDS
        && req.answers.iter().all(|a| {
            words
                .get(a.index)
                .is_some_and(|w| *w == a.word.trim().to_lowercase())
        });

    // A wrong answer is a retry, not a failure: the session stays open so
    // the user can look at their notes again.
    if !all_correct {
        return Err(ApiError::bad_request(
            "those words do not match; check your backup and try again",
        ));
    }

    state.finish(Ok(SetupOutcome {
        phrase,
        kind: SetupKind::Generated,
    }));
    Ok(Json(AddressRes { ok: true }))
}

async fn import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportReq>,
) -> Result<Json<AddressRes>, ApiError> {
    check_token(&state.token, &req.token)?;

    let phrase = normalize_phrase(&req.phrase);
    validate_phrase(&phrase).map_err(|e| ApiError::bad_request(e.to_string()))?;

    state.finish(Ok(SetupOutcome {
        phrase,
        kind: SetupKind::Imported,
    }));
    Ok(Json(AddressRes { ok: true }))
}

async fn cancel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CancelReq>,
) -> Result<StatusCode, ApiError> {
    check_token(&state.token, &req.token)?;
    state.finish(Err(ConnectError::Cancelled(req.reason)));
    Ok(StatusCode::NO_CONTENT)
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
        .route("/generate", post(generate))
        .route("/confirm", post(confirm))
        .route("/import", post(import))
        .route("/cancel", post(cancel))
        .with_state(state);

    serve_and_wait(router, rx, &opts, on_ready).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: &str = "test test test test test test test test test test test junk";

    fn state() -> (
        Arc<AppState>,
        oneshot::Receiver<Result<SetupOutcome, ConnectError>>,
    ) {
        let (tx, rx) = oneshot::channel();
        let s = Arc::new(AppState {
            token: "tok".into(),
            pending: Mutex::new(None),
            tx: Mutex::new(Some(tx)),
        });
        (s, rx)
    }

    fn answers(phrase: &str, idx: &[usize]) -> Vec<ConfirmAnswer> {
        let words: Vec<&str> = phrase.split(' ').collect();
        idx.iter()
            .map(|i| ConfirmAnswer {
                index: *i,
                word: words[*i].to_owned(),
            })
            .collect()
    }

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

    #[tokio::test]
    async fn confirming_the_right_words_completes_setup() {
        let (st, rx) = state();
        generate(
            State(st.clone()),
            Json(GenerateReq {
                token: "tok".into(),
                words: 24,
            }),
        )
        .await
        .unwrap();

        let phrase = st.pending.lock().unwrap().clone().unwrap();
        let res = confirm(
            State(st),
            Json(ConfirmReq {
                token: "tok".into(),
                answers: answers(&phrase, &[0, 5, 11]),
            }),
        )
        .await;
        assert!(res.is_ok());

        let outcome = rx.await.unwrap().unwrap();
        assert_eq!(outcome.kind, SetupKind::Generated);
        assert_eq!(*outcome.phrase, *phrase);
    }

    #[tokio::test]
    async fn wrong_confirmation_words_do_not_complete_setup() {
        let (st, mut rx) = state();
        generate(
            State(st.clone()),
            Json(GenerateReq {
                token: "tok".into(),
                words: 24,
            }),
        )
        .await
        .unwrap();

        let res = confirm(
            State(st),
            Json(ConfirmReq {
                token: "tok".into(),
                answers: vec![
                    ConfirmAnswer {
                        index: 0,
                        word: "wrong".into(),
                    },
                    ConfirmAnswer {
                        index: 1,
                        word: "wrong".into(),
                    },
                    ConfirmAnswer {
                        index: 2,
                        word: "wrong".into(),
                    },
                ],
            }),
        )
        .await;

        assert!(res.is_err());
        // The session must stay open so the user can retry.
        assert!(rx.try_recv().is_err());
    }

    /// Answering fewer positions than asked must not pass by vacuous truth.
    #[tokio::test]
    async fn a_short_answer_set_is_rejected() {
        let (st, _rx) = state();
        generate(
            State(st.clone()),
            Json(GenerateReq {
                token: "tok".into(),
                words: 24,
            }),
        )
        .await
        .unwrap();
        let phrase = st.pending.lock().unwrap().clone().unwrap();

        let res = confirm(
            State(st),
            Json(ConfirmReq {
                token: "tok".into(),
                answers: answers(&phrase, &[0]),
            }),
        )
        .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn confirming_before_generating_is_rejected() {
        let (st, _rx) = state();
        let res = confirm(
            State(st),
            Json(ConfirmReq {
                token: "tok".into(),
                answers: vec![ConfirmAnswer {
                    index: 0,
                    word: "x".into(),
                }],
            }),
        )
        .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn importing_a_valid_phrase_completes_setup() {
        let (st, rx) = state();
        let res = import(
            State(st),
            Json(ImportReq {
                token: "tok".into(),
                phrase: format!("  TEST  {}  ", &KNOWN[5..]),
            }),
        )
        .await;
        assert!(res.is_ok());

        let outcome = rx.await.unwrap().unwrap();
        assert_eq!(outcome.kind, SetupKind::Imported);
        assert_eq!(*outcome.phrase, KNOWN);
    }

    #[tokio::test]
    async fn importing_a_bad_phrase_keeps_the_session_open() {
        let (st, mut rx) = state();
        let res = import(
            State(st),
            Json(ImportReq {
                token: "tok".into(),
                phrase: "abandon abandon abandon".into(),
            }),
        )
        .await;

        assert!(res.is_err());
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn a_bad_token_is_rejected_everywhere() {
        let (st, _rx) = state();
        assert!(
            generate(
                State(st.clone()),
                Json(GenerateReq {
                    token: "wrong".into(),
                    words: 24,
                })
            )
            .await
            .is_err()
        );
        assert!(
            import(
                State(st),
                Json(ImportReq {
                    token: "wrong".into(),
                    phrase: KNOWN.into(),
                })
            )
            .await
            .is_err()
        );
    }
}
