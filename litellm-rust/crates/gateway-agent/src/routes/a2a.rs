use axum::Router;
use axum::routing::{get, post};

use super::not_implemented;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/a2a/:agent_id", post(not_implemented))
        .route("/a2a/:agent_id/message/send", post(not_implemented))
        .route("/v1/a2a/:agent_id/message/send", post(not_implemented))
        .route(
            "/a2a/:agent_id/.well-known/agent-card.json",
            get(not_implemented),
        )
        .route(
            "/a2a/:agent_id/.well-known/agent.json",
            get(not_implemented),
        )
}
