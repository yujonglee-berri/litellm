use axum::Router;
use axum::routing::{get, post};

use super::not_implemented;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/assistants", get(not_implemented).post(not_implemented))
        .route("/v1/assistants", get(not_implemented).post(not_implemented))
        .route(
            "/assistants/*assistant_id",
            axum::routing::delete(not_implemented),
        )
        .route(
            "/v1/assistants/*assistant_id",
            axum::routing::delete(not_implemented),
        )
        .route("/threads", post(not_implemented))
        .route("/v1/threads", post(not_implemented))
        .route("/threads/:thread_id", get(not_implemented))
        .route("/v1/threads/:thread_id", get(not_implemented))
        .route(
            "/threads/:thread_id/messages",
            get(not_implemented).post(not_implemented),
        )
        .route(
            "/v1/threads/:thread_id/messages",
            get(not_implemented).post(not_implemented),
        )
        .route("/threads/:thread_id/runs", post(not_implemented))
        .route("/v1/threads/:thread_id/runs", post(not_implemented))
}
