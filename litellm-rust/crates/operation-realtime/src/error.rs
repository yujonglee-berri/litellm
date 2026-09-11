#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("invalid provider: {0}")]
    InvalidProvider(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("{0}")]
    Auth(String),
    #[error("upstream network error: {0}")]
    Network(String),
    #[error("routing error: {0}")]
    Routing(String),
}

impl From<litellm_auth::AuthError> for Error {
    fn from(error: litellm_auth::AuthError) -> Self {
        Self::Auth(error.to_string())
    }
}
