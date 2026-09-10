use std::time::Duration;

use thiserror::Error as ThisError;

#[derive(Clone, Debug, PartialEq, Eq, ThisError)]
pub enum Error {
    #[error("no deployment available for model '{model}'")]
    NoDeployment { model: String },

    #[error("no deployment can admit model '{model}'")]
    AdmissionRejected {
        model: String,
        retry_after: Option<Duration>,
    },

    #[error("routing state operation failed: {0}")]
    State(String),
}
