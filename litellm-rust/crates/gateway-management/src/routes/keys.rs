use axum::Extension;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use litellm_gateway_auth::RequireMasterKey;
use serde::de::DeserializeOwned;

use crate::keys::{
    BlockKeyRequest, DeleteKeyRequest, GenerateKeyRequest, KeyManager, ListAliasesQuery,
    ListKeysQuery, RegenerateKeyRequest, ResetSpendRequest, UpdateKeyRequest,
};
use crate::state::AppState;

const STUB_GET_ROUTES: &[&str] = &["/credentials/migrate-encryption/check"];
const STUB_POST_ROUTES: &[&str] = &[
    "/key/bulk_update",
    "/team/key/bulk_update",
    "/credentials/migrate-encryption",
];

pub fn router() -> Router<AppState> {
    let router = Router::new()
        .route("/key/generate", post(generate))
        .route(
            "/key/service-account/generate",
            post(generate_service_account),
        )
        .route("/key/info", get(info))
        .route("/v2/key/info", post(info_v2))
        .route("/key/list", get(list))
        .route("/key/aliases", get(aliases))
        .route("/key/update", post(update))
        .route("/key/delete", post(delete))
        .route("/key/regenerate", post(regenerate))
        .route("/key/:key/regenerate", post(regenerate_path))
        .route("/key/:key/reset_spend", post(reset_spend))
        .route("/key/block", post(block))
        .route("/key/unblock", post(unblock))
        .route("/key/health", post(health));
    let router = STUB_GET_ROUTES.iter().fold(router, |router, path| {
        router.route(path, get(not_implemented))
    });
    STUB_POST_ROUTES.iter().fold(router, |router, path| {
        router.route(path, post(not_implemented))
    })
}

async fn generate(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    body: JsonOrDefault<GenerateKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.generate(body.0, Utc::now())?))
}

async fn generate_service_account(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    body: JsonOrDefault<GenerateKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.generate_service_account(body.0, Utc::now())?))
}

async fn info(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    Query(query): Query<InfoQuery>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.info(query.key.as_deref())?))
}

async fn info_v2(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    body: JsonOrDefault<DeleteKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.info_many(body.0)?))
}

async fn list(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    Query(query): Query<ListKeysQuery>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.list(query, Utc::now())?))
}

async fn aliases(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    Query(query): Query<ListAliasesQuery>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.aliases(query)?))
}

async fn update(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    body: JsonOrDefault<UpdateKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.update(body.0, Utc::now())?))
}

async fn delete(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    body: JsonOrDefault<DeleteKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.delete(body.0)?))
}

async fn regenerate(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    body: JsonOrDefault<RegenerateKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.regenerate(None, body.0, Utc::now())?))
}

async fn regenerate_path(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    Path(key): Path<String>,
    body: JsonOrDefault<RegenerateKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.regenerate(Some(&key), body.0, Utc::now())?))
}

async fn reset_spend(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    Path(key): Path<String>,
    Json(request): Json<ResetSpendRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.reset_spend(&key, request, Utc::now())?))
}

async fn block(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    Json(request): Json<BlockKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.set_blocked(request, true, Utc::now())?))
}

async fn unblock(
    _auth: RequireMasterKey,
    Extension(keys): Extension<KeyManager>,
    Json(request): Json<BlockKeyRequest>,
) -> Result<impl IntoResponse, crate::error::Error> {
    Ok(Json(keys.set_blocked(request, false, Utc::now())?))
}

async fn health(
    Extension(keys): Extension<KeyManager>,
    _auth: RequireMasterKey,
) -> impl IntoResponse {
    Json(keys.health())
}

async fn not_implemented(_auth: RequireMasterKey) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

#[derive(serde::Deserialize)]
struct InfoQuery {
    key: Option<String>,
}

struct JsonOrDefault<T>(T);

#[axum::async_trait]
impl<T, S> axum::extract::FromRequest<S> for JsonOrDefault<T>
where
    T: Default + DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(
        req: axum::extract::Request,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
            .await
            .map_err(|error| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": {
                            "message": error.to_string(),
                            "type": "bad_request_error",
                            "code": "400",
                        }
                    })),
                )
                    .into_response()
            })?;
        if bytes.is_empty() {
            return Ok(Self(T::default()));
        }
        serde_json::from_slice(&bytes).map(Self).map_err(|error| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": {
                        "message": error.to_string(),
                        "type": "bad_request_error",
                        "code": "422",
                    }
                })),
            )
                .into_response()
        })
    }
}
