pub mod authorize;
pub mod error;
pub mod setup;
pub mod typed_data;

pub use error::ConnectError;

use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use axum::{
    Json, Router,
    http::{StatusCode, header},
    response::IntoResponse,
};
use tokio::{net::TcpListener, sync::oneshot};

pub struct ConnectOptions {
    pub timeout: Duration,
    pub open_browser: bool,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            open_browser: true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

/// Pages and payloads here carry secrets, so nothing may be written to disk
/// by an intermediary or replayed from a back button.
pub(crate) fn no_store<T: IntoResponse>(body: T) -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-store, max-age=0")], body)
}

pub(crate) fn session_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("system entropy source unavailable");
    hex::encode(buf)
}

pub(crate) fn check_token(expected: &str, got: &str) -> Result<(), ApiError> {
    if expected == got {
        Ok(())
    } else {
        Err(ApiError::new(StatusCode::FORBIDDEN, "bad session token"))
    }
}

/// Serves `router` on loopback until a flow completes, times out, or the
/// browser gives up.
///
/// Shutdown is graceful rather than an abort: the handler that produces the
/// outcome is still writing its response when the outcome arrives, and
/// killing the task here would show the user a connection error on the very
/// last step. The bounded wait keeps a browser holding a keep-alive
/// connection open from stalling the CLI.
pub(crate) async fn serve_and_wait<T>(
    router: Router,
    rx: oneshot::Receiver<Result<T, ConnectError>>,
    opts: &ConnectOptions,
    on_ready: impl FnOnce(&str),
) -> Result<T, ConnectError> {
    // Port 0 on loopback only: never reachable from off-box.
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
    let url = format!("http://127.0.0.1:{}/", listener.local_addr()?.port());

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    on_ready(&url);
    if opts.open_browser && open::that_detached(&url).is_err() {
        server.abort();
        return Err(ConnectError::Browser);
    }

    let outcome = match tokio::time::timeout(opts.timeout, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(ConnectError::Abandoned),
        Err(_) => Err(ConnectError::Timeout),
    };

    let _ = shutdown_tx.send(());
    let abort = server.abort_handle();
    if tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .is_err()
    {
        // A keep-alive connection outlived the flow; the outcome is already
        // in hand, so stop waiting for it.
        abort.abort();
    }

    outcome
}
