use litellm_auth::AuthError;
use thiserror::Error as ThisError;

#[derive(Clone, Debug, ThisError, PartialEq, Eq)]
pub enum Error {
    #[error(transparent)]
    Auth(#[from] AuthError),
}
