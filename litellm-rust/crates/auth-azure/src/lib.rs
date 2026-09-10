//! Azure Entra credential selection and token acquisition.

mod credential_provider_cache;
mod native;
mod resolve;
mod types;

pub use resolve::AzureAuthService;
pub use types::{AzureAuthInputs, AzureCredentialType, ConfigValue, DEFAULT_AZURE_SCOPE};
