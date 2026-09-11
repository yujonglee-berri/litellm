use reqwest::header::{AUTHORIZATION, HeaderName};

use crate::{
    AuthError, AuthResolver, ExistingHeaderBehavior, HeaderAuth, ResolvedAuth, SecretValue,
};

pub trait ApiKeySource {
    fn api_key(&self) -> Option<&SecretValue>;

    fn extra_headers(&self) -> &[(String, String)] {
        &[]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Placement {
    Bearer,
    Header(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiKeyAuth {
    provider: &'static str,
    environment_variable: Option<&'static str>,
    placement: Placement,
}

impl ApiKeyAuth {
    pub const fn bearer(
        provider: &'static str,
        environment_variable: Option<&'static str>,
    ) -> Self {
        Self {
            provider,
            environment_variable,
            placement: Placement::Bearer,
        }
    }

    pub const fn header(
        provider: &'static str,
        environment_variable: Option<&'static str>,
        name: &'static str,
    ) -> Self {
        Self {
            provider,
            environment_variable,
            placement: Placement::Header(name),
        }
    }
}

impl<Call, Services> AuthResolver<Call, Services> for ApiKeyAuth
where
    Call: Sync,
    Services: ApiKeySource + Sync,
{
    type Authenticator = HeaderAuth;
    type AuthContext = ();
    type Error = AuthError;

    async fn resolve(
        &self,
        _input: &Call,
        services: &Services,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, AuthError> {
        let headers = services.extra_headers().to_vec();
        let existing_credential = match self.placement {
            Placement::Bearer => headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(AUTHORIZATION.as_str())),
            Placement::Header(name) => headers
                .iter()
                .any(|(header, _)| header.eq_ignore_ascii_case(name)),
        };
        let environment_key = self
            .environment_variable
            .and_then(|name| std::env::var(name).ok())
            .filter(|value| !value.trim().is_empty())
            .map(SecretValue::new);
        let explicit_key = services
            .api_key()
            .filter(|value| !value.expose().trim().is_empty());
        let key = explicit_key.or(environment_key.as_ref());
        let authenticator = match (key, existing_credential, self.placement) {
            (Some(key), _, Placement::Bearer) => {
                HeaderAuth::bearer(key, ExistingHeaderBehavior::Preserve)?
            }
            (Some(key), _, Placement::Header(name)) => HeaderAuth::new(
                [(header_name(name)?, key.clone())],
                ExistingHeaderBehavior::Preserve,
            )?,
            (None, true, _) => HeaderAuth::new([], ExistingHeaderBehavior::Preserve)?,
            (None, false, _) => {
                return Err(AuthError::MissingApiKey {
                    provider: self.provider,
                });
            }
        };
        Ok(ResolvedAuth {
            authenticator,
            headers,
            context: (),
        })
    }
}

fn header_name(name: &str) -> Result<HeaderName, AuthError> {
    name.parse().map_err(|_| AuthError::InvalidHeader)
}

#[cfg(test)]
mod tests {
    use reqwest::Method;

    use super::*;
    use crate::Auth;

    struct Context {
        key: Option<SecretValue>,
        headers: Vec<(String, String)>,
    }

    impl ApiKeySource for Context {
        fn api_key(&self) -> Option<&SecretValue> {
            self.key.as_ref()
        }

        fn extra_headers(&self) -> &[(String, String)] {
            &self.headers
        }
    }

    #[tokio::test]
    async fn existing_bearer_header_is_a_complete_credential() {
        let context = Context {
            key: None,
            headers: vec![("Authorization".into(), "Bearer caller".into())],
        };
        let resolved = ApiKeyAuth::bearer("Provider", None)
            .resolve(&(), &context)
            .await
            .expect("existing header resolves");
        let mut request = reqwest::Request::new(
            Method::GET,
            "https://provider.test".parse().expect("url parses"),
        );
        request.headers_mut().insert(
            AUTHORIZATION,
            "Bearer caller".parse().expect("header parses"),
        );

        let request = resolved
            .authenticator
            .authenticate(request)
            .await
            .expect("auth preserves header");

        assert_eq!(
            request
                .headers()
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer caller")
        );
    }

    #[tokio::test]
    async fn missing_credential_is_an_auth_foundation_error() {
        let result = ApiKeyAuth::bearer("Mistral", None)
            .resolve(
                &(),
                &Context {
                    key: None,
                    headers: Vec::new(),
                },
            )
            .await;
        let Err(error) = result else {
            panic!("missing key must fail");
        };

        assert!(matches!(
            error,
            AuthError::MissingApiKey {
                provider: "Mistral"
            }
        ));
    }
}
