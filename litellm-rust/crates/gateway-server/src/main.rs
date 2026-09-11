use std::sync::Arc;

#[cfg(feature = "python-config")]
use litellm_config::load_model_list;
use litellm_gateway_inference::integrations::custom_logger::CustomLogger;
use litellm_gateway_inference::integrations::litellm_python_proxy_api::LiteLLMPythonProxyAPILogger;
use litellm_gateway_inference::state::AppState as InferenceState;
use litellm_gateway_router::{Deployment, LiteLLMParams, Router};
#[cfg(feature = "otel")]
use litellm_gateway_server::app_with_otel;
use litellm_gateway_server::{AppState, app};
use litellm_operation_realtime::pool::{PoolConfig, RealtimePool};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 4000;

#[tokio::main]
async fn main() {
    #[cfg(feature = "otel")]
    let _otel = litellm_gateway_otel::Config::from_env()
        .expect("invalid OpenTelemetry configuration")
        .map(litellm_gateway_otel::Runtime::install)
        .transpose()
        .expect("failed to initialize OpenTelemetry");
    let master_key = std::env::var("LITELLM_MASTER_KEY")
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .map(Arc::from);
    let proxy_logger = LiteLLMPythonProxyAPILogger::from_env();
    let loggers: Vec<Arc<dyn CustomLogger>> = vec![proxy_logger];
    let router = Arc::new(build_router());
    let pool_config = PoolConfig::from_env();
    let realtime_pool = RealtimePool::spawn(pool_config);
    if pool_config.enabled() {
        register_deployments(&router, &realtime_pool);
    }
    let state = AppState::new(InferenceState {
        router,
        master_key,
        loggers: Arc::new(loggers),
        realtime_pool,
    });
    let host = std::env::var("HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = tokio::net::TcpListener::bind((host.as_str(), port))
        .await
        .expect("failed to bind listener");

    #[cfg(feature = "otel")]
    let application = if _otel.is_some() {
        app_with_otel(state)
    } else {
        app(state)
    };
    #[cfg(not(feature = "otel"))]
    let application = app(state);

    axum::serve(listener, application)
        .await
        .expect("server error");
}

fn register_deployments(router: &Router, pool: &RealtimePool) {
    for deployment in router.deployments() {
        let params = &deployment.litellm_params;
        let _ = pool.register(
            &params.model,
            params.api_key.as_deref(),
            params.api_base.as_deref(),
        );
    }
}

fn build_router() -> Router {
    #[cfg(feature = "python-config")]
    if let Ok(config_path) = std::env::var("LITELLM_CONFIG_PATH") {
        match load_model_list(std::path::Path::new(&config_path)) {
            Ok(deployments) => return Router::new(deployments),
            Err(error) => eprintln!("config load failed ({error}); falling back to env deployment"),
        }
    }

    let model =
        std::env::var("OPENAI_REALTIME_MODEL").unwrap_or_else(|_| "gpt-realtime".to_string());
    Router::new(vec![Deployment {
        model_name: model.clone(),
        litellm_params: LiteLLMParams {
            model,
            api_key: std::env::var("OPENAI_API_KEY").ok(),
            api_base: None,
        },
    }])
}
