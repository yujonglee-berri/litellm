use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("token endpoint must be HTTPS and contain no embedded credentials or fragment")]
    InvalidTokenEndpoint,
    #[error("extra OAuth field uses reserved form parameter: {0}")]
    ReservedExtraField(String),
    #[error("token endpoint request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("token endpoint returned HTTP {0}")]
    HttpStatus(StatusCode),
    #[error("token endpoint response was invalid: {0}")]
    InvalidResponse(#[source] reqwest::Error),
    #[error("token endpoint returned an empty access token")]
    EmptyAccessToken,
}
