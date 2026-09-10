use axum::Router;
use axum::routing::get;

use super::not_implemented;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/skills", get(not_implemented).post(not_implemented))
        .route(
            "/v1/skills/:skill_id",
            get(not_implemented).delete(not_implemented),
        )
}
