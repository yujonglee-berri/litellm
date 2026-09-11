use std::future::Future;

use crate::{Auth, AuthError, BearerTokenAuth, ExistingHeaderBehavior, TokenProviderHandle};

pub struct ResolvedAuth<A, C> {
    pub authenticator: A,
    pub headers: Vec<(String, String)>,
    pub context: C,
}

pub trait AuthResolver<Input: ?Sized, Services: ?Sized>: Send + Sync {
    type Authenticator: Auth;
    type AuthContext: Send + Sync;
    type Error;

    fn resolve(
        &self,
        input: &Input,
        services: &Services,
    ) -> impl Future<
        Output = Result<ResolvedAuth<Self::Authenticator, Self::AuthContext>, Self::Error>,
    > + Send;
}

#[derive(Clone, Debug)]
pub struct TokenAuthResolver {
    provider: TokenProviderHandle,
    existing: ExistingHeaderBehavior,
}

impl TokenAuthResolver {
    pub fn new(provider: TokenProviderHandle, existing: ExistingHeaderBehavior) -> Self {
        Self { provider, existing }
    }
}

impl<Input, Services> AuthResolver<Input, Services> for TokenAuthResolver
where
    Input: Sync + ?Sized,
    Services: Sync + ?Sized,
{
    type Authenticator = BearerTokenAuth;
    type AuthContext = ();
    type Error = AuthError;

    async fn resolve(
        &self,
        _input: &Input,
        _services: &Services,
    ) -> Result<ResolvedAuth<BearerTokenAuth, ()>, AuthError> {
        Ok(ResolvedAuth {
            authenticator: BearerTokenAuth::new(self.provider.clone(), self.existing),
            headers: Vec::new(),
            context: (),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{Auth, AuthScheme, ResolvedCredential, SecretValue, TokenFuture, TokenProvider};

    use super::*;

    #[derive(Debug)]
    struct StaticToken;

    impl TokenProvider for StaticToken {
        fn acquire(&self) -> TokenFuture<'_> {
            Box::pin(async {
                Ok(ResolvedCredential::Static(SecretValue::new(
                    "resolved-token",
                )))
            })
        }
    }

    #[tokio::test]
    async fn token_resolver_applies_the_provider_token_to_the_final_request() {
        let resolver = TokenAuthResolver::new(
            TokenProviderHandle::new(Arc::new(StaticToken)),
            ExistingHeaderBehavior::Preserve,
        );
        let resolved = resolver.resolve(&(), &()).await.expect("resolver succeeds");
        let request = reqwest::Request::new(
            reqwest::Method::GET,
            "https://provider.test".parse().expect("url parses"),
        );

        let request = resolved
            .authenticator
            .authenticate(request)
            .await
            .expect("token authenticates");

        assert_eq!(resolved.authenticator.scheme(), AuthScheme::Bearer);
        assert_eq!(
            request
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer resolved-token")
        );
    }
}
