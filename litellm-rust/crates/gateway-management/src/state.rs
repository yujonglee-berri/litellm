use std::sync::Arc;

use litellm_gateway_auth::MasterKeyProvider;

#[derive(Clone)]
pub struct AppState {
    pub master_key: Option<Arc<str>>,
}

impl MasterKeyProvider for AppState {
    fn master_key(&self) -> Option<&str> {
        self.master_key.as_deref()
    }
}
