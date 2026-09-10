use axum::Router;
use axum::routing::get;

use super::not_implemented;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1beta/agents", get(not_implemented).post(not_implemented))
        .route(
            "/v1beta/agents/:name",
            get(not_implemented).delete(not_implemented),
        )
        .route("/v1beta/agents/:name/versions", get(not_implemented))
}
