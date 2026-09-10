use std::sync::Arc;

use litellm_gateway_agent::routes;
use litellm_gateway_agent::state::AppState;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 4003;

#[tokio::main]
async fn main() {
    let master_key = std::env::var("LITELLM_MASTER_KEY")
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .map(Arc::from);
    let host = std::env::var("HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = tokio::net::TcpListener::bind((host.as_str(), port))
        .await
        .expect("failed to bind listener");

    axum::serve(listener, routes::app(AppState { master_key }))
        .await
        .expect("server error");
}
