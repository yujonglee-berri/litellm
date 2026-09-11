#[cfg(feature = "server")]
use axum::Json;
#[cfg(feature = "server")]
use axum::http::StatusCode;
#[cfg(feature = "server")]
use axum::response::{IntoResponse, Response};
#[cfg(feature = "server")]
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{message}")]
    BadRequest {
        message: String,
        param: Option<&'static str>,
    },
    #[error("{message}")]
    NotFound {
        message: String,
        param: Option<&'static str>,
    },
    #[error("{message}")]
    Database { message: String },
}

impl Error {
    pub fn bad_request(message: impl Into<String>, param: Option<&'static str>) -> Self {
        Self::BadRequest {
            message: message.into(),
            param,
        }
    }

    pub fn not_found(message: impl Into<String>, param: Option<&'static str>) -> Self {
        Self::NotFound {
            message: message.into(),
            param,
        }
    }

    pub fn database(error: impl ToString) -> Self {
        Self::Database {
            message: error.to_string(),
        }
    }

    #[cfg(feature = "server")]
    fn status_and_type(&self) -> (StatusCode, &'static str, Option<&'static str>) {
        match self {
            Self::BadRequest { param, .. } => {
                (StatusCode::BAD_REQUEST, "bad_request_error", *param)
            }
            Self::NotFound { param, .. } => (StatusCode::NOT_FOUND, "not_found_error", *param),
            Self::Database { .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_server_error",
                None,
            ),
        }
    }
}

#[cfg(feature = "server")]
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error_type, param) = self.status_and_type();
        (
            status,
            Json(json!({
                "error": {
                    "message": self.to_string(),
                    "type": error_type,
                    "param": param,
                    "code": status.as_u16().to_string(),
                }
            })),
        )
            .into_response()
    }
}
