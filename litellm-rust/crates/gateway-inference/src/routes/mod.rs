//! HTTP routes.
//!
//! **Template:** every route module exposes `pub fn router() -> Router<AppState>`
//! that mounts its own paths; [`app`] merges them. A trivial route is a single
//! file (`health.rs`); a non-trivial one is a folder (`realtime/`) with
//! `handler` (entry) + `service` (logic) + `transport` (adapters). See AGENTS.md.

pub mod health;
pub mod messages;
mod porting;
pub mod realtime;
pub mod responses;

use axum::Router;

use crate::state::AppState;

/// Assemble the application router by merging every route module's `router()`.
pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(health::router())
        .merge(messages::router())
        .merge(porting::router())
        .merge(realtime::router())
        .merge(responses::router())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::header::AUTHORIZATION;
    use axum::http::{Request, StatusCode};
    use litellm_gateway_router::Router as ModelRouter;
    use tower::ServiceExt;

    use super::app;
    use crate::state::AppState;
    use litellm_core::realtime::pool::RealtimePool;

    fn state() -> AppState {
        AppState {
            router: Arc::new(ModelRouter::new(Vec::new())),
            master_key: Some(Arc::from("test-key")),
            loggers: Arc::new(Vec::new()),
            realtime_pool: RealtimePool::disabled(),
        }
    }

    async fn status(method: &str, path: &str) -> StatusCode {
        app(state())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header(AUTHORIZATION, "Bearer test-key")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds")
            .status()
    }

    #[tokio::test]
    async fn registers_unported_inference_families() {
        assert_eq!(
            status("POST", "/v1/chat/completions").await,
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(status("POST", "/v1/ocr").await, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(
            status("POST", "/anthropic/v1/messages").await,
            StatusCode::NOT_IMPLEMENTED
        );
    }

    #[tokio::test]
    async fn does_not_register_management_routes() {
        assert_eq!(status("POST", "/key/generate").await, StatusCode::NOT_FOUND);
    }
}
