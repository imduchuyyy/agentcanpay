use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("could not reach the API: {0}")]
    Transport(String),

    #[error("API returned HTTP {status}")]
    Status { status: u16 },

    #[error("API reported failure: {0}")]
    Upstream(String),

    #[error("API response did not match the expected shape: {0}")]
    Decode(String),
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_decode() {
            Self::Decode(e.to_string())
        } else if let Some(status) = e.status() {
            Self::Status {
                status: status.as_u16(),
            }
        } else {
            Self::Transport(e.to_string())
        }
    }
}
