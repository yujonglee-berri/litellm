use axum::Router;
use axum::routing::{any, get, post};

use super::not_implemented;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/mcp", any(not_implemented))
        .route("/mcp/", any(not_implemented))
        .route("/mcp/*path", any(not_implemented))
        .route("/mcp-rest/tools/list", get(not_implemented))
        .route("/mcp-rest/tools/call", post(not_implemented))
        .route("/v1/mcp/tools", get(not_implemented))
        .route(
            "/v1/mcp/oauth/authorize",
            get(not_implemented).post(not_implemented),
        )
        .route("/v1/mcp/oauth/token", post(not_implemented))
        .route("/introspect", post(not_implemented))
        .route("/revoke", post(not_implemented))
        .route("/authorize", get(not_implemented))
        .route("/authorize/flow", get(not_implemented))
        .route("/authorize/complete", post(not_implemented))
        .route("/token", post(not_implemented))
        .route("/register", post(not_implemented))
        .route("/callback", get(not_implemented))
        .route("/:mcp_server_name/authorize", get(not_implemented))
        .route("/:mcp_server_name/token", post(not_implemented))
        .route("/:mcp_server_name/register", post(not_implemented))
        .route(
            "/.well-known/oauth-protected-resource",
            get(not_implemented),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(not_implemented),
        )
        .route("/.well-known/openid-configuration", get(not_implemented))
        .route("/.well-known/jwks.json", get(not_implemented))
        .route("/.well-known/litellm-cli-auth", get(not_implemented))
        .route(
            "/.well-known/oauth-authorization-server/:mcp_server_name/mcp",
            get(not_implemented),
        )
}
