use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("timed out waiting for the browser wallet")]
    Timeout,

    #[error("cancelled in the browser: {0}")]
    Cancelled(String),

    #[error("the browser closed without completing the handshake")]
    Abandoned,

    #[error("malformed signature from the wallet")]
    BadSignature,

    #[error(
        "signature does not match the connected account; smart-contract \
         wallets (e.g. Safe) cannot be used, because they produce no \
         recoverable signature to derive from"
    )]
    AddressMismatch,

    #[error("could not open a browser; re-run with --print-url")]
    Browser,

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
