use axum::Router;
use axum::http::StatusCode;
use axum::routing::any;

use crate::constants::PORTING_MANAGEMENT_ROUTE_GROUPS;
use crate::state::AppState;
use litellm_gateway_auth::RequireMasterKey;

pub fn router() -> Router<AppState> {
    PORTING_MANAGEMENT_ROUTE_GROUPS
        .iter()
        .flat_map(|routes| routes.iter())
        .fold(Router::new(), |router, path| {
            router.route(path, any(not_implemented))
        })
}

async fn not_implemented(_auth: RequireMasterKey) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
