#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("authentication failed: {0}")]
    Auth(#[from] litellm_auth::AuthError),
    #[error("transport failed: {0}")]
    Transport(#[from] litellm_transport::Error),
    #[error("provider rejected the request with status {status}: {message}")]
    Provider { status: u16, message: String },
    #[error("adapter failed: {0}")]
    Adapter(#[from] litellm_operation::Error),
}
