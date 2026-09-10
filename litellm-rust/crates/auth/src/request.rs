use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Request;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};

use crate::error::AuthConfigurationError;
use crate::{AuthError, ExistingHeaderBehavior, SecretValue, TokenProviderHandle};

pub type AuthFuture<'a> = Pin<Box<dyn Future<Output = Result<Request, AuthError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthScheme {
    None,
    Header,
    Bearer,
    Basic,
    Query,
    AwsSigV4,
    Composite,
}

pub trait Auth: Send + Sync {
    fn scheme(&self) -> AuthScheme;

    fn authenticate(&self, request: Request) -> AuthFuture<'_>;
}

#[derive(Clone)]
pub struct AuthHandle(Arc<dyn Auth>);

impl AuthHandle {
    pub fn new(auth: impl Auth + 'static) -> Self {
        Self(Arc::new(auth))
    }
}

impl std::fmt::Debug for AuthHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("AuthHandle")
            .field(&"[REDACTED]")
            .finish()
    }
}

impl Auth for AuthHandle {
    fn scheme(&self) -> AuthScheme {
        self.0.scheme()
    }

    fn authenticate(&self, request: Request) -> AuthFuture<'_> {
        self.0.authenticate(request)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoAuth;

impl Auth for NoAuth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::None
    }

    fn authenticate(&self, request: Request) -> AuthFuture<'_> {
        Box::pin(async move { Ok(request) })
    }
}

#[derive(Clone, Debug)]
pub struct HeaderAuth {
    headers: HeaderMap,
    existing: ExistingHeaderBehavior,
    scheme: AuthScheme,
}

impl HeaderAuth {
    pub fn new(
        headers: impl IntoIterator<Item = (HeaderName, SecretValue)>,
        existing: ExistingHeaderBehavior,
    ) -> Result<Self, AuthError> {
        let mut values = HeaderMap::new();
        for (name, secret) in headers {
            if values.contains_key(&name) {
                return Err(AuthConfigurationError::ExistingCredentialHeader.into());
            }
            if secret.expose().trim().is_empty() {
                return Err(AuthConfigurationError::EmptyCredential.into());
            }
            let mut value =
                HeaderValue::from_str(secret.expose()).map_err(|_| AuthError::InvalidHeader)?;
            value.set_sensitive(true);
            values.insert(name, value);
        }
        Ok(Self {
            headers: values,
            existing,
            scheme: AuthScheme::Header,
        })
    }

    pub fn bearer(
        token: &SecretValue,
        existing: ExistingHeaderBehavior,
    ) -> Result<Self, AuthError> {
        if token.expose().trim().is_empty() {
            return Err(AuthConfigurationError::EmptyCredential.into());
        }
        Self::new(
            [(
                AUTHORIZATION,
                SecretValue::new(format!("Bearer {}", token.expose())),
            )],
            existing,
        )
        .map(|auth| Self {
            scheme: AuthScheme::Bearer,
            ..auth
        })
    }

    pub fn apply(&self, mut request: Request) -> Result<Request, AuthError> {
        for (name, value) in &self.headers {
            if preserve_existing(request.headers_mut(), name, self.existing)? {
                continue;
            }
            request.headers_mut().insert(name, value.clone());
        }
        Ok(request)
    }
}

impl Auth for HeaderAuth {
    fn scheme(&self) -> AuthScheme {
        self.scheme
    }

    fn authenticate(&self, request: Request) -> AuthFuture<'_> {
        Box::pin(async move { self.apply(request) })
    }
}

#[derive(Clone, Debug)]
pub struct BearerTokenAuth {
    provider: TokenProviderHandle,
    existing: ExistingHeaderBehavior,
}

impl BearerTokenAuth {
    pub fn new(provider: TokenProviderHandle, existing: ExistingHeaderBehavior) -> Self {
        Self { provider, existing }
    }
}

impl Auth for BearerTokenAuth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::Bearer
    }

    fn authenticate(&self, mut request: Request) -> AuthFuture<'_> {
        Box::pin(async move {
            if preserve_existing(request.headers_mut(), &AUTHORIZATION, self.existing)? {
                return Ok(request);
            }
            let credential = self.provider.acquire().await?;
            HeaderAuth::bearer(credential.secret(), self.existing)?.apply(request)
        })
    }
}

#[derive(Clone, Debug)]
pub struct BasicAuth {
    username: SecretValue,
    password: Option<SecretValue>,
    existing: ExistingHeaderBehavior,
}

impl BasicAuth {
    pub fn new(
        username: SecretValue,
        password: Option<SecretValue>,
        existing: ExistingHeaderBehavior,
    ) -> Result<Self, AuthError> {
        if username.expose().trim().is_empty() {
            return Err(AuthConfigurationError::EmptyBasicUsername.into());
        }
        Ok(Self {
            username,
            password,
            existing,
        })
    }
}

impl Auth for BasicAuth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::Basic
    }

    fn authenticate(&self, request: Request) -> AuthFuture<'_> {
        Box::pin(async move {
            let password = self
                .password
                .as_ref()
                .map(SecretValue::expose)
                .unwrap_or_default();
            let encoded = STANDARD.encode(format!("{}:{password}", self.username.expose()));
            HeaderAuth::new(
                [(AUTHORIZATION, SecretValue::new(format!("Basic {encoded}")))],
                self.existing,
            )?
            .apply(request)
        })
    }
}

#[derive(Clone, Debug)]
pub struct QueryParamAuth {
    name: String,
    value: SecretValue,
    existing: ExistingHeaderBehavior,
}

impl QueryParamAuth {
    pub fn new(
        name: impl Into<String>,
        value: SecretValue,
        existing: ExistingHeaderBehavior,
    ) -> Result<Self, AuthError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(AuthConfigurationError::EmptyCredentialName.into());
        }
        if value.expose().trim().is_empty() {
            return Err(AuthConfigurationError::EmptyCredential.into());
        }
        Ok(Self {
            name,
            value,
            existing,
        })
    }
}

impl Auth for QueryParamAuth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::Query
    }

    fn authenticate(&self, mut request: Request) -> AuthFuture<'_> {
        Box::pin(async move {
            let exists = request
                .url()
                .query_pairs()
                .any(|(name, _)| name == self.name);
            if exists {
                match self.existing {
                    ExistingHeaderBehavior::Preserve => return Ok(request),
                    ExistingHeaderBehavior::Reject => {
                        return Err(AuthConfigurationError::DuplicateQueryParameter(
                            self.name.clone(),
                        )
                        .into());
                    }
                    ExistingHeaderBehavior::Replace => {
                        let retained = request
                            .url()
                            .query_pairs()
                            .filter(|(name, _)| name.as_ref() != self.name)
                            .map(|(name, value)| (name.into_owned(), value.into_owned()))
                            .collect::<Vec<_>>();
                        request.url_mut().set_query(None);
                        request.url_mut().query_pairs_mut().extend_pairs(retained);
                    }
                }
            }
            request
                .url_mut()
                .query_pairs_mut()
                .append_pair(&self.name, self.value.expose());
            Ok(request)
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct CompositeAuth {
    authenticators: Vec<AuthHandle>,
}

impl CompositeAuth {
    pub fn new(authenticators: impl IntoIterator<Item = AuthHandle>) -> Self {
        Self {
            authenticators: authenticators.into_iter().collect(),
        }
    }
}

impl Auth for CompositeAuth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::Composite
    }

    fn authenticate(&self, request: Request) -> AuthFuture<'_> {
        Box::pin(async move {
            let mut request = request;
            for authenticator in &self.authenticators {
                request = authenticator.authenticate(request).await?;
            }
            Ok(request)
        })
    }
}

fn preserve_existing(
    headers: &mut HeaderMap,
    name: &HeaderName,
    behavior: ExistingHeaderBehavior,
) -> Result<bool, AuthError> {
    if !headers.contains_key(name) {
        return Ok(false);
    }
    match behavior {
        ExistingHeaderBehavior::Reject => {
            return Err(AuthConfigurationError::ExistingCredentialHeader.into());
        }
        ExistingHeaderBehavior::Replace => {
            headers.remove(name);
            return Ok(false);
        }
        ExistingHeaderBehavior::Preserve => {}
    }
    for (_, value) in headers.iter_mut().filter(|(key, _)| *key == name) {
        value.set_sensitive(true);
    }
    Ok(true)
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
