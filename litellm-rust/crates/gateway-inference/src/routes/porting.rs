use axum::Router;
use axum::http::StatusCode;
use axum::routing::{any, get, post};

use crate::constants::{PORTING_GET_ROUTES, PORTING_PASSTHROUGH_ROUTES, PORTING_POST_ROUTES};
use crate::state::AppState;
use litellm_gateway_auth::RequireMasterKey;

pub fn router() -> Router<AppState> {
    let get_routes = PORTING_GET_ROUTES
        .iter()
        .fold(Router::new(), |router, path| {
            router.route(path, get(not_implemented))
        });
    let post_routes = PORTING_POST_ROUTES.iter().fold(get_routes, |router, path| {
        router.route(path, post(not_implemented))
    });
    PORTING_PASSTHROUGH_ROUTES
        .iter()
        .fold(post_routes, |router, path| {
            router.route(path, any(not_implemented))
        })
}

async fn not_implemented(_auth: RequireMasterKey) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
