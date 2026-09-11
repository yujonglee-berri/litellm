mod health;
mod keys;
mod porting;

use axum::Extension;
use axum::Router;

use crate::keys::KeyManager;
use crate::state::AppState;

/// Standalone process: local health plus this family's contribution.
/// The process is the host here, so it injects the key manager.
pub fn app(state: AppState, keys: KeyManager) -> Router {
    Router::new()
        .merge(health::router())
        .merge(family())
        .layer(Extension(keys))
        .with_state(state)
}

/// Domain routes this family contributes to the host.
/// Process health stays on the host, or on [`app`] when this binary runs alone.
pub fn contribute(state: AppState) -> Router {
    family().with_state(state)
}

fn family() -> Router<AppState> {
    Router::new().merge(keys::router()).merge(porting::router())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
    use axum::http::{Request, StatusCode};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use axum::Extension;

    use super::{app, contribute};
    use crate::keys::{KeyManager, MemoryStore};
    use crate::state::AppState;

    fn state() -> AppState {
        AppState::new(Some("test-key".into()))
    }

    fn keys() -> KeyManager {
        KeyManager::memory()
    }

    async fn send(
        state: AppState,
        keys: KeyManager,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(AUTHORIZATION, "Bearer test-key");
        if body.is_some() {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        let request = builder
            .body(match body {
                Some(value) => Body::from(serde_json::to_vec(&value).expect("json")),
                None => Body::empty(),
            })
            .expect("request builds");
        let response = app(state, keys)
            .oneshot(request)
            .await
            .expect("router responds");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, json)
    }

    #[tokio::test]
    async fn does_not_register_inference_routes() {
        let (status, _) = send(state(), keys(), "POST", "/v1/chat/completions", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn generate_list_info_update_and_delete_are_real() {
        let state = state();
        let keys = keys();
        let (status, generated) = send(
            state.clone(),
            keys.clone(),
            "POST",
            "/key/generate",
            Some(json!({
                "key_alias": "prod",
                "models": ["gpt-4"],
                "max_budget": 12.5,
                "user_id": "alice"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let key = generated["key"].as_str().expect("plaintext key");
        assert!(key.starts_with("sk-"));
        assert_eq!(generated["token"], key);
        assert_ne!(generated["token_id"], key);
        assert_eq!(generated["key_alias"], "prod");
        assert_eq!(generated["max_budget"], 12.5);

        let (status, info) = send(
            state.clone(),
            keys.clone(),
            "GET",
            &format!("/key/info?key={key}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(info["key"], key);
        assert_eq!(info["info"]["key_alias"], "prod");
        assert!(info["info"].get("token").is_none());
        assert_ne!(info["info"]["token_id"], key);

        let (status, listed) = send(
            state.clone(),
            keys.clone(),
            "GET",
            "/key/list?user_id=alice",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed["total_count"], 1);
        assert_eq!(listed["keys"][0], generated["token_id"]);

        let (status, updated) = send(
            state.clone(),
            keys.clone(),
            "POST",
            "/key/update",
            Some(json!({
                "key_alias": "prod",
                "max_budget": 50.0,
                "blocked": true
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(updated["max_budget"], 50.0);
        assert_eq!(updated["blocked"], true);

        let (status, deleted) = send(
            state.clone(),
            keys.clone(),
            "POST",
            "/key/delete",
            Some(json!({ "keys": [key] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(deleted["deleted_keys"], json!([key]));

        let (status, _) = send(state, keys, "GET", &format!("/key/info?key={key}"), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn regenerate_and_block_rotate_the_secret() {
        let state = state();
        let keys = keys();
        let (_, generated) = send(
            state.clone(),
            keys.clone(),
            "POST",
            "/key/generate",
            Some(json!({})),
        )
        .await;
        let key = generated["key"].as_str().expect("key").to_string();

        let (status, blocked) = send(
            state.clone(),
            keys.clone(),
            "POST",
            "/key/block",
            Some(json!({ "key": key })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(blocked["blocked"], true);

        let (status, rotated) = send(
            state.clone(),
            keys.clone(),
            "POST",
            "/key/regenerate",
            Some(json!({ "key": key })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let new_key = rotated["key"].as_str().expect("new key");
        assert_ne!(new_key, key);
        assert_eq!(rotated["rotation_count"], 1);

        let (status, _) = send(
            state.clone(),
            keys.clone(),
            "GET",
            &format!("/key/info?key={key}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = send(
            state,
            keys,
            "GET",
            &format!("/key/info?key={new_key}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn remaining_credential_routes_stay_unported() {
        let state = state();
        let keys = keys();
        for path in ["/credentials/migrate-encryption/check"] {
            let (status, _) = send(state.clone(), keys.clone(), "GET", path, None).await;
            assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{path}");
        }
        for path in [
            "/key/bulk_update",
            "/team/key/bulk_update",
            "/credentials/migrate-encryption",
        ] {
            let (status, _) = send(state.clone(), keys.clone(), "POST", path, None).await;
            assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{path}");
        }
        let (status, _) = send(state, keys, "GET", "/key/generate", None).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn key_routes_require_master_key() {
        let response = app(state(), keys())
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

    #[tokio::test]
    async fn host_owns_health_family_contributes_keys() {
        let contributed = contribute(state());
        let standalone = app(state(), keys());
        let contribute_health = contributed
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health/liveness")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");
        let standalone_health = standalone
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health/liveness")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");
        assert_eq!(contribute_health.status(), StatusCode::NOT_FOUND);
        assert_eq!(standalone_health.status(), StatusCode::OK);

        let (status, _) = send(state(), keys(), "POST", "/key/generate", Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn contribute_uses_the_memory_store_the_host_injects() {
        let keys = KeyManager::new(MemoryStore::default());
        let other = KeyManager::new(MemoryStore::default());
        let router = contribute(state()).layer(Extension(keys.clone()));

        let generate = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/key/generate")
                    .header(AUTHORIZATION, "Bearer test-key")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"key_alias":"injected"}"#))
                    .expect("request builds"),
            )
            .await
            .expect("router responds");
        assert_eq!(generate.status(), StatusCode::OK);
        let generated: Value = serde_json::from_slice(
            &axum::body::to_bytes(generate.into_body(), usize::MAX)
                .await
                .expect("body"),
        )
        .expect("json");
        let key = generated["key"].as_str().expect("plaintext key");

        let found = keys.info(Some(key)).expect("same store");
        assert_eq!(found.info.key_alias.as_deref(), Some("injected"));
        assert!(other.info(Some(key)).is_err());

        let missing = contribute(state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/key/generate")
                    .header(AUTHORIZATION, "Bearer test-key")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .expect("request builds"),
            )
            .await
            .expect("router responds");
        assert_eq!(missing.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
