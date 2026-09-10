use std::future::Future;
use std::pin::Pin;

use reqwest::Request;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};

use super::error::AuthConfigurationError;
use super::{AuthError, ExistingHeaderBehavior, SecretValue, TokenProviderHandle};

pub type AuthFuture<'a> = Pin<Box<dyn Future<Output = Result<Request, AuthError>> + Send + 'a>>;

pub trait Auth: Send + Sync {
    fn authenticate(&self, request: Request) -> AuthFuture<'_>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoAuth;

impl Auth for NoAuth {
    fn authenticate(&self, request: Request) -> AuthFuture<'_> {
        Box::pin(async move { Ok(request) })
    }
}

#[derive(Clone, Debug)]
pub struct HeaderAuth {
    headers: HeaderMap,
    existing: ExistingHeaderBehavior,
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

fn preserve_existing(
    headers: &mut HeaderMap,
    name: &HeaderName,
    behavior: ExistingHeaderBehavior,
) -> Result<bool, AuthError> {
    if !headers.contains_key(name) {
        return Ok(false);
    }
    if behavior == ExistingHeaderBehavior::Reject {
        return Err(AuthConfigurationError::ExistingCredentialHeader.into());
    }
    for (_, value) in headers.iter_mut().filter(|(key, _)| *key == name) {
        value.set_sensitive(true);
    }
    Ok(true)
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
