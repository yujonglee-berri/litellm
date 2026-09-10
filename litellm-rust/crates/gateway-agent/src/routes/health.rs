use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health/liveness", get(ok))
        .route("/health/readiness", get(ok))
}

async fn ok() -> StatusCode {
    StatusCode::OK
}
