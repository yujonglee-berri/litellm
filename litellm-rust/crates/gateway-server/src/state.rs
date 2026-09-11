use std::sync::Arc;

use litellm_gateway_agent::state::AppState as AgentState;
use litellm_gateway_inference::integrations::custom_logger::CustomLogger;
use litellm_gateway_inference::state::AppState as InferenceState;
use litellm_gateway_management::keys::KeyManager;
use litellm_gateway_management::state::AppState as ManagementState;
use litellm_gateway_router::Router;
use litellm_operation_realtime::pool::RealtimePool;
use litellm_persist::SqliteStore;

#[derive(Clone)]
pub struct AppState {
    pub inference: InferenceState,
    pub agent: AgentState,
    pub management: ManagementState,
    pub keys: KeyManager,
}

impl AppState {
    pub fn new(inference: InferenceState) -> Self {
        let master_key = inference.master_key.clone();
        Self {
            inference,
            agent: AgentState {
                master_key: master_key.clone(),
            },
            management: ManagementState::new(master_key),
            keys: key_database_from_env(),
        }
    }

    pub fn empty(master_key: Option<Arc<str>>) -> Self {
        Self {
            inference: InferenceState {
                router: Arc::new(Router::new(Vec::new())),
                master_key: master_key.clone(),
                loggers: Arc::new(Vec::<Arc<dyn CustomLogger>>::new()),
                realtime_pool: RealtimePool::disabled(),
            },
            agent: AgentState {
                master_key: master_key.clone(),
            },
            management: ManagementState::new(master_key),
            keys: KeyManager::memory(),
        }
    }
}

fn key_database_from_env() -> KeyManager {
    let path =
        std::env::var("LITELLM_DATABASE_PATH").unwrap_or_else(|_| "litellm.sqlite".to_string());
    KeyManager::new(SqliteStore::open(path).expect("open key database"))
}
