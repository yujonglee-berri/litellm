use litellm_auth::{ExistingHeaderBehavior, HeaderAuth, ResolveAuth, ResolvedAuth, SecretValue};
use reqwest::header::HeaderName;

use crate::Error;

pub trait ApiKeySource {
    fn api_key(&self) -> Option<&SecretValue>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Placement {
    Bearer,
    Header(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiKeyAuth {
    provider: &'static str,
    placement: Placement,
}

impl ApiKeyAuth {
    pub const fn bearer(provider: &'static str) -> Self {
        Self {
            provider,
            placement: Placement::Bearer,
        }
    }

    pub const fn header(provider: &'static str, name: &'static str) -> Self {
        Self {
            provider,
            placement: Placement::Header(name),
        }
    }
}

fn credential_header_name(name: &str) -> Result<HeaderName, Error> {
    name.parse()
        .map_err(|_| Error::InvalidRequest(format!("invalid credential header name: {name}")))
}

impl<Call, Context> ResolveAuth<Call, Context> for ApiKeyAuth
where
    Call: Sync,
    Context: ApiKeySource + Sync,
{
    type Authenticator = HeaderAuth;
    type Context = ();
    type Error = Error;

    async fn resolve(
        &self,
        _call: &Call,
        context: &Context,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, Error> {
        let key = context
            .api_key()
            .ok_or(Error::MissingApiKey(self.provider))?;
        let authenticator = match self.placement {
            Placement::Bearer => HeaderAuth::bearer(key, ExistingHeaderBehavior::Preserve)?,
            Placement::Header(name) => HeaderAuth::new(
                [(credential_header_name(name)?, key.clone())],
                ExistingHeaderBehavior::Preserve,
            )?,
        };
        Ok(ResolvedAuth {
            authenticator,
            headers: Vec::new(),
            context: (),
        })
    }
}

#[cfg(test)]
mod tests {
    use litellm_auth::{Auth, SecretValue};
    use reqwest::Method;

    use super::*;

    struct StaticKey(Option<SecretValue>);

    impl ApiKeySource for StaticKey {
        fn api_key(&self) -> Option<&SecretValue> {
            self.0.as_ref()
        }
    }

    fn context(key: &str) -> StaticKey {
        StaticKey(Some(SecretValue::new(key)))
    }

    async fn applied_headers(
        auth: &ApiKeyAuth,
        key_context: &StaticKey,
    ) -> reqwest::header::HeaderMap {
        let resolved = auth
            .resolve(&(), key_context)
            .await
            .expect("context key resolves");
        let request = reqwest::Request::new(Method::GET, "https://provider.test/".parse().unwrap());
        let authenticated = resolved
            .authenticator
            .authenticate(request)
            .await
            .expect("auth applies to the final request");
        authenticated.headers().clone()
    }

    #[tokio::test]
    async fn bearer_sends_the_context_key_as_authorization() {
        let headers = applied_headers(&ApiKeyAuth::bearer("mistral"), &context("test-key")).await;

        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-key")
        );
    }

    #[tokio::test]
    async fn header_sends_the_context_key_under_the_named_header() {
        let auth = ApiKeyAuth::header("azure", "ocp-apim-subscription-key");

        let headers = applied_headers(&auth, &context("test-key")).await;

        assert_eq!(
            headers
                .get("ocp-apim-subscription-key")
                .and_then(|value| value.to_str().ok()),
            Some("test-key")
        );
    }

    #[tokio::test]
    async fn missing_key_names_the_provider() {
        let error = ApiKeyAuth::bearer("reducto")
            .resolve(&(), &StaticKey(None))
            .await
            .err()
            .expect("missing key fails");

        assert!(matches!(error, Error::MissingApiKey("reducto")));
    }

    #[tokio::test]
    async fn blank_key_is_rejected_rather_than_sent() {
        let error = ApiKeyAuth::bearer("mistral")
            .resolve(&(), &context(" "))
            .await
            .err()
            .expect("blank key fails");

        assert!(matches!(error, Error::Auth(_)));
    }
}
