use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health/liveness", get(liveness))
        .route("/health/readiness", get(readiness))
}

async fn liveness() -> StatusCode {
    StatusCode::OK
}

async fn readiness() -> StatusCode {
    StatusCode::OK
}
