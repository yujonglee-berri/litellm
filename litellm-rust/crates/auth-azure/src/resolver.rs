use std::sync::Arc;

use litellm_auth::{
    ApiKeyAuth, ApiKeySource, AuthError, AuthHandle, AuthResolver, BearerTokenAuth,
    ExistingHeaderBehavior, ResolvedAuth, TokenAuthResolver, TokenFuture, TokenProvider,
    TokenProviderHandle,
};

use crate::{AzureAuthInputs, AzureAuthService};

type EnvironmentLookup = dyn Fn(&str) -> Option<String> + Send + Sync;

#[derive(Clone)]
pub struct AzureTokenProvider {
    service: Arc<AzureAuthService>,
    inputs: AzureAuthInputs,
    environment: Arc<EnvironmentLookup>,
}

impl AzureTokenProvider {
    pub fn new(inputs: AzureAuthInputs, environment: Arc<EnvironmentLookup>) -> Self {
        Self::with_service(Arc::new(AzureAuthService::default()), inputs, environment)
    }

    pub fn with_service(
        service: Arc<AzureAuthService>,
        inputs: AzureAuthInputs,
        environment: Arc<EnvironmentLookup>,
    ) -> Self {
        Self {
            service,
            inputs,
            environment,
        }
    }

    pub fn into_handle(self) -> TokenProviderHandle {
        TokenProviderHandle::new(Arc::new(self))
    }
}

impl std::fmt::Debug for AzureTokenProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AzureTokenProvider")
            .field("service", &"[REDACTED]")
            .field("inputs", &self.inputs)
            .field("environment", &"[REDACTED]")
            .finish()
    }
}

impl TokenProvider for AzureTokenProvider {
    fn acquire(&self) -> TokenFuture<'_> {
        Box::pin(async move {
            self.service
                .get_azure_ad_token(&self.inputs, self.environment.as_ref())
                .await?
                .map(|credential| credential.into_value())
                .ok_or(AuthError::TokenAcquisition {
                    mechanism: "Azure Entra",
                    message: "no credential was selected".to_string(),
                })
        })
    }
}

#[derive(Clone, Debug)]
pub struct AzureEntraAuthResolver {
    inner: TokenAuthResolver,
}

impl AzureEntraAuthResolver {
    pub fn new(provider: TokenProviderHandle) -> Self {
        Self {
            inner: TokenAuthResolver::new(provider, ExistingHeaderBehavior::Replace),
        }
    }

    pub fn from_inputs(inputs: AzureAuthInputs, environment: Arc<EnvironmentLookup>) -> Self {
        Self::new(AzureTokenProvider::new(inputs, environment).into_handle())
    }
}

impl<Input, Services> AuthResolver<Input, Services> for AzureEntraAuthResolver
where
    Input: Sync + ?Sized,
    Services: Sync + ?Sized,
{
    type Authenticator = BearerTokenAuth;
    type AuthContext = ();
    type Error = AuthError;

    async fn resolve(
        &self,
        input: &Input,
        services: &Services,
    ) -> Result<ResolvedAuth<BearerTokenAuth, ()>, AuthError> {
        self.inner.resolve(input, services).await
    }
}

#[derive(Clone, Debug)]
pub struct AzureAuthResolver {
    api_key: ApiKeyAuth,
    entra: AzureEntraAuthResolver,
}

impl AzureAuthResolver {
    pub fn new(api_key: ApiKeyAuth, entra: AzureEntraAuthResolver) -> Self {
        Self { api_key, entra }
    }
}

impl<Input, Services> AuthResolver<Input, Services> for AzureAuthResolver
where
    Input: Sync,
    Services: ApiKeySource + Sync,
{
    type Authenticator = AuthHandle;
    type AuthContext = ();
    type Error = AuthError;

    async fn resolve(
        &self,
        input: &Input,
        services: &Services,
    ) -> Result<ResolvedAuth<AuthHandle, ()>, AuthError> {
        match self.api_key.resolve(input, services).await {
            Ok(resolved) => Ok(ResolvedAuth {
                authenticator: AuthHandle::new(resolved.authenticator),
                headers: resolved.headers,
                context: (),
            }),
            Err(AuthError::MissingApiKey { .. }) => {
                let resolved = self.entra.resolve(input, services).await?;
                Ok(ResolvedAuth {
                    authenticator: AuthHandle::new(resolved.authenticator),
                    headers: services.extra_headers().to_vec(),
                    context: (),
                })
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use litellm_auth::{Auth, AuthScheme, ResolvedCredential, SecretValue, TokenProvider};
    use reqwest::header::{AUTHORIZATION, HeaderName};

    use super::*;

    #[derive(Debug)]
    struct CountingToken {
        calls: AtomicUsize,
    }

    impl TokenProvider for CountingToken {
        fn acquire(&self) -> TokenFuture<'_> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(ResolvedCredential::AccessToken {
                    token: SecretValue::new("entra-token"),
                    expires_on: None,
                })
            })
        }
    }

    struct Services {
        api_key: Option<SecretValue>,
        headers: Vec<(String, String)>,
    }

    impl ApiKeySource for Services {
        fn api_key(&self) -> Option<&SecretValue> {
            self.api_key.as_ref()
        }

        fn extra_headers(&self) -> &[(String, String)] {
            &self.headers
        }
    }

    fn request() -> reqwest::Request {
        reqwest::Request::new(
            reqwest::Method::GET,
            "https://provider.test".parse().unwrap(),
        )
    }

    fn resolver(token: Arc<CountingToken>) -> AzureAuthResolver {
        AzureAuthResolver::new(
            ApiKeyAuth::header("Azure", None, "api-key"),
            AzureEntraAuthResolver::new(TokenProviderHandle::new(token)),
        )
    }

    #[tokio::test]
    async fn entra_resolver_applies_injected_provider_token() {
        let token = Arc::new(CountingToken {
            calls: AtomicUsize::new(0),
        });
        let resolved = AzureEntraAuthResolver::new(TokenProviderHandle::new(token.clone()))
            .resolve(&(), &())
            .await
            .unwrap();

        let request = resolved
            .authenticator
            .authenticate(request())
            .await
            .unwrap();

        assert_eq!(resolved.authenticator.scheme(), AuthScheme::Bearer);
        assert_eq!(
            request
                .headers()
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer entra-token")
        );
        assert_eq!(token.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn api_key_takes_precedence_without_acquiring_entra_token() {
        let token = Arc::new(CountingToken {
            calls: AtomicUsize::new(0),
        });
        let resolved = resolver(token.clone())
            .resolve(
                &(),
                &Services {
                    api_key: Some(SecretValue::new("key-value")),
                    headers: Vec::new(),
                },
            )
            .await
            .unwrap();

        let request = resolved
            .authenticator
            .authenticate(request())
            .await
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get(HeaderName::from_static("api-key"))
                .and_then(|value| value.to_str().ok()),
            Some("key-value")
        );
        assert_eq!(request.headers().get(AUTHORIZATION), None);
        assert_eq!(token.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn untrusted_authorization_does_not_bypass_entra_and_is_replaced() {
        let token = Arc::new(CountingToken {
            calls: AtomicUsize::new(0),
        });
        let resolved = resolver(token.clone())
            .resolve(
                &(),
                &Services {
                    api_key: None,
                    headers: vec![
                        ("Authorization".to_string(), "Bearer untrusted".to_string()),
                        ("x-trace".to_string(), "trace-id".to_string()),
                    ],
                },
            )
            .await
            .unwrap();

        assert_eq!(
            resolved.headers,
            [
                ("Authorization".to_string(), "Bearer untrusted".to_string()),
                ("x-trace".to_string(), "trace-id".to_string()),
            ]
        );
        let mut request = request();
        request.headers_mut().insert(
            AUTHORIZATION,
            "Bearer untrusted".parse().expect("header parses"),
        );
        let request = resolved.authenticator.authenticate(request).await.unwrap();

        assert_eq!(
            request
                .headers()
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer entra-token")
        );
        assert_eq!(token.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn azure_token_adapter_preserves_service_selection_precedence() {
        let caller = Arc::new(CountingToken {
            calls: AtomicUsize::new(0),
        });
        let inputs = AzureAuthInputs {
            azure_ad_token: crate::ConfigValue::Value(litellm_auth::Sourced::new(
                SecretValue::new("supplied-token"),
                litellm_auth::InputSource::Deployment,
            )),
            azure_ad_token_provider: Some(TokenProviderHandle::new(caller.clone())),
            ..Default::default()
        };
        let provider = AzureTokenProvider::new(inputs, Arc::new(|_| None));

        let credential = provider.acquire().await.unwrap();

        assert_eq!(credential.secret().expose(), "entra-token");
        assert_eq!(caller.calls.load(Ordering::SeqCst), 1);
    }
}
