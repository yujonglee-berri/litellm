mod health;
mod keys;
mod porting;

use axum::Router;

use crate::state::AppState;

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(health::router())
        .merge(keys::router())
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

    #[tokio::test]
    async fn registers_key_routes_with_python_methods() {
        let state = AppState {
            master_key: Some("test-key".into()),
        };
        for path in [
            "/key/info",
            "/key/list",
            "/key/aliases",
            "/credentials/migrate-encryption/check",
        ] {
            let response = app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(path)
                        .header(AUTHORIZATION, "Bearer test-key")
                        .body(Body::empty())
                        .expect("request builds"),
                )
                .await
                .expect("router responds");

            assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
        }
        for path in [
            "/key/generate",
            "/key/service-account/generate",
            "/key/update",
            "/key/bulk_update",
            "/team/key/bulk_update",
            "/key/delete",
            "/v2/key/info",
            "/credentials/migrate-encryption",
            "/key/example/regenerate",
            "/key/regenerate",
            "/key/example/reset_spend",
            "/key/block",
            "/key/unblock",
            "/key/health",
        ] {
            let response = app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header(AUTHORIZATION, "Bearer test-key")
                        .body(Body::empty())
                        .expect("request builds"),
                )
                .await
                .expect("router responds");

            assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
        }
        let wrong_method_response = app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/key/generate")
                    .header(AUTHORIZATION, "Bearer test-key")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(
            wrong_method_response.status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
    }

    #[tokio::test]
    async fn key_routes_require_master_key() {
        let response = app(AppState {
            master_key: Some("test-key".into()),
        })
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/key/block")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
