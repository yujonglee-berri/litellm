#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("invalid provider: {0}")]
    InvalidProvider(String),
    #[error("{0}")]
    Auth(String),
    #[error("upstream network error: {0}")]
    Network(String),
    #[error("provider rejected the request with status {status}")]
    Http { status: u16, body: String },
}

impl From<litellm_auth::AuthError> for Error {
    fn from(error: litellm_auth::AuthError) -> Self {
        Self::Auth(error.to_string())
    }
}
