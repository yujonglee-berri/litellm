#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Database(String),
}

impl Error {
    pub fn database(error: impl ToString) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<Error> for litellm_gateway_management::error::Error {
    fn from(error: Error) -> Self {
        Self::database(error)
    }
}
