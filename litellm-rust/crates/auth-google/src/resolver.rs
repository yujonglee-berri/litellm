use std::sync::Arc;

use litellm_auth::{
    AuthError, AuthResolver, BearerTokenAuth, ExistingHeaderBehavior, ResolvedAuth,
    TokenAuthResolver, TokenProviderHandle,
};

use crate::{GoogleAuthContext, GoogleAuthInputs, GoogleTokenAcquirer, GoogleTokenProvider};

pub trait GoogleAuthSource: Sync {
    fn google_auth_inputs(&self) -> &GoogleAuthInputs;
    fn google_token_backend(&self) -> &Arc<dyn GoogleTokenAcquirer>;
    fn google_existing_header_behavior(&self) -> ExistingHeaderBehavior;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GoogleAuthResolver;

impl<Call, Services> AuthResolver<Call, Services> for GoogleAuthResolver
where
    Call: Sync + ?Sized,
    Services: GoogleAuthSource + ?Sized,
{
    type Authenticator = BearerTokenAuth;
    type AuthContext = GoogleAuthContext;
    type Error = AuthError;

    async fn resolve(
        &self,
        _call: &Call,
        services: &Services,
    ) -> Result<ResolvedAuth<Self::Authenticator, Self::AuthContext>, Self::Error> {
        let input = services.google_auth_inputs();
        let provider = GoogleTokenProvider::new(
            input.token_request.clone(),
            services.google_token_backend().clone(),
        );
        let token_resolver = TokenAuthResolver::new(
            TokenProviderHandle::new(Arc::new(provider)),
            services.google_existing_header_behavior(),
        );
        let resolved = token_resolver.resolve(_call, services).await?;

        Ok(ResolvedAuth {
            authenticator: resolved.authenticator,
            headers: resolved.headers,
            context: GoogleAuthContext {
                project: input.project.clone(),
                location: input.location.clone(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use litellm_auth::{
        Auth, ExistingHeaderBehavior, ResolvedCredential, SecretValue, TokenFuture,
    };

    use super::*;
    use crate::{GoogleCredentialSource, GoogleTokenAcquirer, GoogleTokenRequest};

    struct SemanticCall {
        prompt: String,
    }

    struct Services {
        inputs: GoogleAuthInputs,
        backend: Arc<dyn GoogleTokenAcquirer>,
    }

    impl GoogleAuthSource for Services {
        fn google_auth_inputs(&self) -> &GoogleAuthInputs {
            &self.inputs
        }

        fn google_token_backend(&self) -> &Arc<dyn GoogleTokenAcquirer> {
            &self.backend
        }

        fn google_existing_header_behavior(&self) -> ExistingHeaderBehavior {
            ExistingHeaderBehavior::Replace
        }
    }

    #[derive(Debug)]
    struct UnusedBackend;

    impl GoogleTokenAcquirer for UnusedBackend {
        fn acquire<'a>(&'a self, _request: &'a GoogleTokenRequest) -> TokenFuture<'a> {
            Box::pin(async { panic!("supplied token must bypass backend") })
        }
    }

    #[tokio::test]
    async fn arbitrary_call_gets_bearer_auth_and_endpoint_context() {
        let call = SemanticCall {
            prompt: "unrelated semantic input".into(),
        };
        let services = Services {
            inputs: GoogleAuthInputs {
                token_request: GoogleTokenRequest {
                    source: GoogleCredentialSource::AccessToken(ResolvedCredential::Static(
                        SecretValue::new("google-token"),
                    )),
                    scopes: vec![],
                    audience: None,
                    quota_project_id: None,
                },
                project: Some("project-a".into()),
                location: Some("us-central1".into()),
            },
            backend: Arc::new(UnusedBackend),
        };
        let resolved = GoogleAuthResolver.resolve(&call, &services).await.unwrap();
        let request = reqwest::Request::new(
            reqwest::Method::POST,
            "https://vertex.test".parse().unwrap(),
        );

        let authenticated = resolved.authenticator.authenticate(request).await.unwrap();

        assert_eq!(
            authenticated
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .unwrap(),
            "Bearer google-token"
        );
        assert_eq!(call.prompt, "unrelated semantic input");
        assert_eq!(
            resolved.context,
            GoogleAuthContext {
                project: Some("project-a".into()),
                location: Some("us-central1".into()),
            }
        );
    }
}
