#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request authentication failed: {0}")]
    Authentication(#[from] litellm_auth::AuthError),
    #[error("request execution failed: {0}")]
    Execution(#[from] reqwest::Error),
}
