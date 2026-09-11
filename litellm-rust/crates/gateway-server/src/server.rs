use axum::Extension;
use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use litellm_gateway_agent::routes as agent;
use litellm_gateway_inference::routes as inference;
use litellm_gateway_management::routes as management;

use crate::AppState;

pub fn app(state: AppState) -> Router {
    compose(state)
}

#[cfg(feature = "otel")]
pub fn app_with_otel(state: AppState) -> Router {
    litellm_gateway_otel::instrument(compose(state))
}

fn compose(state: AppState) -> Router {
    Router::new()
        .route("/health/liveness", get(ok))
        .route("/health/readiness", get(ok))
        .merge(inference::contribute(state.inference))
        .merge(agent::contribute(state.agent))
        .merge(management::contribute(state.management))
        .layer(Extension(state.keys))
}

async fn ok() -> StatusCode {
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::header::AUTHORIZATION;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::app;
    use crate::AppState;

    async fn status(method: &str, path: &str) -> StatusCode {
        app(AppState::empty(Some(Arc::from("test-key"))))
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
    async fn composes_all_gateway_route_families() {
        assert_eq!(status("GET", "/health/liveness").await, StatusCode::OK);
        assert_eq!(
            status("POST", "/v1/chat/completions").await,
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(status("POST", "/mcp").await, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(status("POST", "/key/generate").await, StatusCode::OK);
    }
}
