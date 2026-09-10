mod health;
mod porting;

use axum::Router;

use crate::state::AppState;

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(health::router())
        .merge(porting::router())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::header::AUTHORIZATION;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::app;
    use crate::state::AppState;

    #[tokio::test]
    async fn does_not_register_inference_routes() {
        let response = app(AppState {
            master_key: Some("test-key".into()),
        })
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn registers_unported_management_families() {
        let response = app(AppState {
            master_key: Some("test-key".into()),
        })
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/key/generate")
                .header(AUTHORIZATION, "Bearer test-key")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }
}
