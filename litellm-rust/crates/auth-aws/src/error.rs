use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error(transparent)]
    Auth(#[from] litellm_auth::AuthError),
}
