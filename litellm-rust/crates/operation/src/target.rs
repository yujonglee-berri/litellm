use litellm_auth::{Auth, ResolveAuth, ResolvedAuth};

use crate::Error;
use crate::endpoint::ResolveEndpoint;

#[derive(Debug)]
pub struct HttpTarget<A> {
    pub endpoint: reqwest::Url,
    pub authenticator: A,
}

#[derive(Debug)]
pub struct TargetAuth<A> {
    authenticator: A,
}

impl<A> TargetAuth<A> {
    pub fn new(authenticator: A) -> Self {
        Self { authenticator }
    }
}

impl<A, Call, Context> ResolveAuth<Call, Context> for TargetAuth<A>
where
    A: Auth + Clone,
    Call: Send + Sync,
    Context: Send + Sync,
{
    type Authenticator = A;
    type Context = ();
    type Error = Error;

    async fn resolve(
        &self,
        _input: &Call,
        _services: &Context,
    ) -> Result<ResolvedAuth<A, ()>, Error> {
        Ok(ResolvedAuth {
            authenticator: self.authenticator.clone(),
            headers: Vec::new(),
            context: (),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetEndpoint {
    endpoint: reqwest::Url,
}

impl TargetEndpoint {
    pub fn new(endpoint: reqwest::Url) -> Self {
        Self { endpoint }
    }
}

impl<AuthContext, Params, Call, Context> ResolveEndpoint<AuthContext, Params, Call, Context>
    for TargetEndpoint
{
    fn resolve(
        &self,
        _call: &Call,
        _context: &Context,
        _auth: &AuthContext,
        _params: &Params,
    ) -> Result<String, Error> {
        Ok(self.endpoint.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use litellm_auth::{Auth, AuthFuture, AuthScheme};

    use super::*;

    #[derive(Clone)]
    struct UnitAuth;

    impl Auth for UnitAuth {
        fn scheme(&self) -> AuthScheme {
            AuthScheme::None
        }

        fn authenticate(&self, request: reqwest::Request) -> AuthFuture<'_> {
            Box::pin(async move { Ok(request) })
        }
    }

    #[tokio::test]
    async fn target_auth_returns_the_supplied_authenticator() {
        let resolved = TargetAuth::new(UnitAuth)
            .resolve(&"call", &())
            .await
            .expect("target auth resolves");

        assert!(resolved.headers.is_empty());
        assert_eq!(resolved.authenticator.scheme(), AuthScheme::None);
    }

    #[test]
    fn target_endpoint_uses_the_full_operation_url() {
        let endpoint = TargetEndpoint::new(
            "https://openrouter.ai/api/v1/messages"
                .parse()
                .expect("url parses"),
        );

        let resolved = ResolveEndpoint::<(), (), (), ()>::resolve(&endpoint, &(), &(), &(), &())
            .expect("target endpoint resolves");

        assert_eq!(resolved, "https://openrouter.ai/api/v1/messages");
    }
}
