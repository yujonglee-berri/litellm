use axum::Router;
use axum::routing::get;

use super::not_implemented;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/memory", get(not_implemented).post(not_implemented))
        .route(
            "/v1/memory/*key",
            get(not_implemented)
                .put(not_implemented)
                .delete(not_implemented),
        )
}
