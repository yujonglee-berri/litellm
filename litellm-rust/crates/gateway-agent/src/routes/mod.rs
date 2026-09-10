mod a2a;
mod agents;
mod assistants;
mod health;
mod mcp;
mod memory;
mod skills;

use axum::Router;
use axum::http::StatusCode;

use crate::extractors::identity::Identity;
use crate::state::AppState;

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(a2a::router())
        .merge(agents::router())
        .merge(assistants::router())
        .merge(health::router())
        .merge(memory::router())
        .merge(mcp::router())
        .merge(skills::router())
        .with_state(state)
}

async fn not_implemented(_identity: Identity) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::header::AUTHORIZATION;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::app;
    use crate::state::AppState;

    async fn status(method: &str, path: &str) -> StatusCode {
        app(AppState {
            master_key: Some("test-key".into()),
        })
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
    async fn registers_agent_data_plane_routes() {
        assert_eq!(status("POST", "/mcp").await, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(
            status("POST", "/a2a/example").await,
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            status("GET", "/v1/skills").await,
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            status("GET", "/v1beta/agents").await,
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            status("POST", "/v1/threads/thread-1/runs").await,
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            status("PUT", "/v1/memory/key-1").await,
            StatusCode::NOT_IMPLEMENTED
        );
    }

    #[tokio::test]
    async fn excludes_inference_and_management_routes() {
        assert_eq!(
            status("POST", "/v1/chat/completions").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(status("POST", "/v1/agents").await, StatusCode::NOT_FOUND);
        assert_eq!(
            status("POST", "/v1/mcp/server").await,
            StatusCode::NOT_FOUND
        );
    }
}
