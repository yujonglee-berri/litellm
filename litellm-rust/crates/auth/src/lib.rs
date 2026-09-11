//! Operation-independent outbound authentication contracts.
//!
//! Provider crates resolve credentials and choose an authenticator. Routes hand the
//! authenticator the final HTTP request, after URL and body transformation.

mod api_key;
mod credential;
pub mod error;
mod http;
mod policy;
mod request;
mod resolver;
mod secret;
mod token;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    Request,
    #[default]
    Deployment,
    Environment,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Sourced<T> {
    value: T,
    source: InputSource,
}

impl<T> Sourced<T> {
    pub fn new(value: T, source: InputSource) -> Self {
        Self { value, source }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn source(&self) -> InputSource {
        self.source
    }

    pub fn into_value(self) -> T {
        self.value
    }

    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Sourced<U> {
        Sourced::new(map(self.value), self.source)
    }
}

pub use api_key::{ApiKeyAuth, ApiKeySource};
pub use credential::{
    CredentialFileRef, CredentialLookup, CredentialLookupFuture, CredentialPlan,
    CredentialPlanResolution, CredentialRef, CredentialResolver, CredentialResolverHandle,
};
pub use error::{AuthConfigurationError, AuthError, AwsAuthError, MissingCredential};
pub use http::CredentialPlacement;
pub use policy::{CredentialPlanKind, CredentialRule, ExistingHeaderBehavior, ProviderAuthPolicy};
pub use request::{
    Auth, AuthFuture, AuthHandle, AuthScheme, BasicAuth, BearerTokenAuth, CompositeAuth,
    HeaderAuth, NoAuth, QueryParamAuth,
};
pub use resolver::{AuthResolver, ResolvedAuth, TokenAuthResolver};
pub use secret::SecretValue;
pub use token::{
    CachedTokenProvider, ResolvedCredential, TokenFuture, TokenProvider, TokenProviderHandle,
};
