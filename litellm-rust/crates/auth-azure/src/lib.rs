//! Azure Entra credential selection and token acquisition.

mod credential_provider_cache;
mod error;
mod native;
mod resolve;
mod resolver;
mod types;

pub use error::Error;
pub use resolve::AzureAuthService;
pub use resolver::{
    AzureAuthContext, AzureAuthResolver, AzureEntraAuthResolver, AzureTokenProvider,
};
pub use types::{AzureAuthInputs, AzureCredentialType, ConfigValue, DEFAULT_AZURE_SCOPE};
