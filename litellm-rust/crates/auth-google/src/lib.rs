//! Typed Google Cloud credential inputs.
//!
//! SDK-specific backends implement `GoogleTokenAcquirer`; provider operations only see the
//! shared `TokenProvider` contract.

use std::sync::Arc;

use litellm_auth::{
    AuthError, ResolvedCredential, SecretValue, TokenFuture, TokenProvider, TokenProviderHandle,
};

#[derive(Clone, Debug)]
pub enum GoogleCredentialSource {
    AccessToken(ResolvedCredential),
    ServiceAccountJson(SecretValue),
    ApplicationDefault,
    AuthorizedUserJson(SecretValue),
    ExternalAccountJson(SecretValue),
    AwsWorkloadIdentity(SecretValue),
    PluggableExternalAccount(SecretValue),
    Caller(TokenProviderHandle),
}

#[derive(Clone, Debug)]
pub struct GoogleTokenRequest {
    pub source: GoogleCredentialSource,
    pub scopes: Vec<String>,
    pub audience: Option<String>,
    pub quota_project_id: Option<String>,
}

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
    use std::time::SystemTime;

    use super::*;

    #[tokio::test]
    async fn supplied_access_token_does_not_call_the_cloud_backend() {
        let expected = ResolvedCredential::AccessToken {
            token: SecretValue::new("token"),
            expires_on: Some(SystemTime::now()),
        };
        let provider = GoogleTokenProvider::new(
            GoogleTokenRequest {
                source: GoogleCredentialSource::AccessToken(expected.clone()),
                scopes: vec!["scope".into()],
                audience: None,
                quota_project_id: None,
            },
            Arc::new(UnsupportedGoogleBackend),
        );

        assert_eq!(provider.acquire().await.unwrap(), expected);
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
