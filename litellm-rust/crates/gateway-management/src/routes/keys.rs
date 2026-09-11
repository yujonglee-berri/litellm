use axum::Router;
use axum::http::StatusCode;
use axum::routing::{get, post};
use litellm_gateway_auth::RequireMasterKey;

use crate::state::AppState;

const GET_ROUTES: &[&str] = &[
    "/key/info",
    "/key/list",
    "/key/aliases",
    "/credentials/migrate-encryption/check",
];

const POST_ROUTES: &[&str] = &[
    "/key/generate",
    "/key/service-account/generate",
    "/key/update",
    "/key/bulk_update",
    "/team/key/bulk_update",
    "/key/delete",
    "/v2/key/info",
    "/credentials/migrate-encryption",
    "/key/:key/regenerate",
    "/key/regenerate",
    "/key/:key/reset_spend",
    "/key/block",
    "/key/unblock",
    "/key/health",
];

pub fn router() -> Router<AppState> {
    let get_routes = GET_ROUTES.iter().fold(Router::new(), |router, path| {
        router.route(path, get(not_implemented))
    });
    POST_ROUTES.iter().fold(get_routes, |router, path| {
        router.route(path, post(not_implemented))
    })
}

async fn not_implemented(_auth: RequireMasterKey) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
