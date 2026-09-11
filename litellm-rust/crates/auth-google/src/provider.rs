use std::sync::Arc;

use litellm_auth::{AuthError, TokenFuture, TokenProvider};

use crate::{GoogleCredentialSource, GoogleTokenRequest};

pub trait GoogleTokenAcquirer: std::fmt::Debug + Send + Sync {
    fn acquire<'a>(&'a self, request: &'a GoogleTokenRequest) -> TokenFuture<'a>;
}

#[derive(Clone, Debug)]
pub struct GoogleTokenProvider {
    request: GoogleTokenRequest,
    backend: Arc<dyn GoogleTokenAcquirer>,
}

impl GoogleTokenProvider {
    pub fn new(request: GoogleTokenRequest, backend: Arc<dyn GoogleTokenAcquirer>) -> Self {
        Self { request, backend }
    }
}

impl TokenProvider for GoogleTokenProvider {
    fn acquire(&self) -> TokenFuture<'_> {
        Box::pin(async move {
            match &self.request.source {
                GoogleCredentialSource::AccessToken(token) => Ok(token.clone()),
                GoogleCredentialSource::Caller(provider) => provider.acquire().await,
                GoogleCredentialSource::ServiceAccountJson(_)
                | GoogleCredentialSource::ApplicationDefault
                | GoogleCredentialSource::AuthorizedUserJson(_)
                | GoogleCredentialSource::ExternalAccountJson(_)
                | GoogleCredentialSource::AwsWorkloadIdentity(_)
                | GoogleCredentialSource::PluggableExternalAccount(_) => {
                    self.backend.acquire(&self.request).await
                }
            }
        })
    }
}

#[derive(Debug)]
pub struct UnsupportedGoogleBackend;

impl GoogleTokenAcquirer for UnsupportedGoogleBackend {
    fn acquire<'a>(&'a self, _request: &'a GoogleTokenRequest) -> TokenFuture<'a> {
        Box::pin(async {
            Err(AuthError::TokenAcquisition {
                mechanism: "Google Cloud",
                message: "no Google credential backend is installed".into(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use litellm_auth::{ResolvedCredential, SecretValue, TokenProvider, TokenProviderHandle};

    use super::*;

    #[derive(Debug)]
    struct CountingAcquirer {
        calls: AtomicUsize,
        token: &'static str,
    }

    impl GoogleTokenAcquirer for CountingAcquirer {
        fn acquire<'a>(&'a self, _request: &'a GoogleTokenRequest) -> TokenFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(ResolvedCredential::Static(SecretValue::new(self.token))) })
        }
    }

    #[derive(Debug)]
    struct CallerProvider {
        calls: AtomicUsize,
    }

    impl TokenProvider for CallerProvider {
        fn acquire(&self) -> TokenFuture<'_> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(ResolvedCredential::Static(SecretValue::new("caller"))) })
        }
    }

    fn request(source: GoogleCredentialSource) -> GoogleTokenRequest {
        GoogleTokenRequest {
            source,
            scopes: vec!["scope".into()],
            audience: None,
            quota_project_id: None,
        }
    }

    #[tokio::test]
    async fn supplied_access_token_bypasses_backend() {
        let backend = Arc::new(CountingAcquirer {
            calls: AtomicUsize::new(0),
            token: "backend",
        });
        let expected = ResolvedCredential::Static(SecretValue::new("supplied"));
        let provider = GoogleTokenProvider::new(
            request(GoogleCredentialSource::AccessToken(expected.clone())),
            backend.clone(),
        );

        assert_eq!(provider.acquire().await.unwrap(), expected);
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn caller_provider_bypasses_backend() {
        let backend = Arc::new(CountingAcquirer {
            calls: AtomicUsize::new(0),
            token: "backend",
        });
        let caller = Arc::new(CallerProvider {
            calls: AtomicUsize::new(0),
        });
        let provider = GoogleTokenProvider::new(
            request(GoogleCredentialSource::Caller(TokenProviderHandle::new(
                caller.clone(),
            ))),
            backend.clone(),
        );

        assert_eq!(
            provider.acquire().await.unwrap().secret().expose(),
            "caller"
        );
        assert_eq!(caller.calls.load(Ordering::SeqCst), 1);
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn google_credentials_use_backend() {
        let backend = Arc::new(CountingAcquirer {
            calls: AtomicUsize::new(0),
            token: "backend",
        });
        let provider = GoogleTokenProvider::new(
            request(GoogleCredentialSource::ApplicationDefault),
            backend.clone(),
        );

        assert_eq!(
            provider.acquire().await.unwrap().secret().expose(),
            "backend"
        );
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn credential_json_is_redacted_for_every_source() {
        let sources = [
            GoogleCredentialSource::ServiceAccountJson(SecretValue::new("private-service-json")),
            GoogleCredentialSource::AuthorizedUserJson(SecretValue::new("private-user-json")),
            GoogleCredentialSource::ExternalAccountJson(SecretValue::new("private-external-json")),
            GoogleCredentialSource::AwsWorkloadIdentity(SecretValue::new("private-aws-json")),
            GoogleCredentialSource::PluggableExternalAccount(SecretValue::new("private-exec-json")),
        ];

        assert!(
            sources
                .iter()
                .all(|source| !format!("{source:?}").contains("private"))
        );
    }
}
