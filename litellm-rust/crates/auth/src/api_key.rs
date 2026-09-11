use reqwest::header::{AUTHORIZATION, HeaderName};

use crate::{
    AuthError, AuthResolver, CredentialLookup, CredentialLookupFuture, CredentialPlan,
    CredentialPlanResolution, CredentialRef, CredentialResolver, ExistingHeaderBehavior,
    HeaderAuth, ResolvedAuth, SecretValue,
};

pub trait ApiKeySource {
    fn api_key(&self) -> Option<&SecretValue>;

    fn credential_lookup<'a>(&'a self, reference: &'a CredentialRef) -> CredentialLookupFuture<'a>
    where
        Self: Sync,
    {
        Box::pin(async move {
            Ok(match reference {
                CredentialRef::Request(name) if name == "api_key" => self
                    .api_key()
                    .cloned()
                    .map_or(CredentialLookup::Missing, CredentialLookup::Found),
                _ => CredentialLookup::Declined,
            })
        })
    }

    fn extra_headers(&self) -> &[(String, String)] {
        &[]
    }

    fn sdk_credential_lookup<'a>(&'a self, _name: &'a str) -> CredentialLookupFuture<'a>
    where
        Self: Sync,
    {
        Box::pin(async { Ok(CredentialLookup::Declined) })
    }
}

struct ApiKeyResolver<'a, T>(&'a T);

impl<T> std::fmt::Debug for ApiKeyResolver<'_, T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ApiKeyResolver([REDACTED])")
    }
}

impl<T> CredentialResolver for ApiKeyResolver<'_, T>
where
    T: ApiKeySource + Send + Sync,
{
    fn resolve<'a>(&'a self, reference: &'a CredentialRef) -> CredentialLookupFuture<'a> {
        self.0.credential_lookup(reference)
    }

    fn resolve_sdk<'a>(&'a self, name: &'a str) -> CredentialLookupFuture<'a> {
        self.0.sdk_credential_lookup(name)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExistingCredentialPolicy {
    Accept,
    #[default]
    Reject,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApiKeyAuthContext {
    pub source: Option<crate::CredentialSource>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Placement {
    Bearer,
    Header(&'static str),
}

#[derive(Clone, Debug)]
pub struct ApiKeyAuth {
    provider: &'static str,
    plan: CredentialPlan,
    placement: Placement,
    existing_credential: ExistingCredentialPolicy,
}

impl ApiKeyAuth {
    pub fn bearer(provider: &'static str, plan: CredentialPlan) -> Self {
        Self {
            provider,
            plan,
            placement: Placement::Bearer,
            existing_credential: ExistingCredentialPolicy::Reject,
        }
    }

    pub fn header(provider: &'static str, plan: CredentialPlan, name: &'static str) -> Self {
        Self {
            provider,
            plan,
            placement: Placement::Header(name),
            existing_credential: ExistingCredentialPolicy::Reject,
        }
    }

    pub fn existing_credential_policy(mut self, policy: ExistingCredentialPolicy) -> Self {
        self.existing_credential = policy;
        self
    }
}

impl<Call, Services> AuthResolver<Call, Services> for ApiKeyAuth
where
    Call: Sync,
    Services: ApiKeySource + Send + Sync,
{
    type Authenticator = HeaderAuth;
    type AuthContext = ApiKeyAuthContext;
    type Error = AuthError;

    async fn resolve(
        &self,
        _input: &Call,
        services: &Services,
    ) -> Result<ResolvedAuth<HeaderAuth, ApiKeyAuthContext>, AuthError> {
        let headers = services.extra_headers().to_vec();
        let existing_credential = match self.placement {
            Placement::Bearer => headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(AUTHORIZATION.as_str())),
            Placement::Header(name) => headers
                .iter()
                .any(|(header, _)| header.eq_ignore_ascii_case(name)),
        };
        let resolution = self.plan.resolve(&ApiKeyResolver(services)).await?;
        let existing_behavior = match self.existing_credential {
            ExistingCredentialPolicy::Accept => ExistingHeaderBehavior::Preserve,
            ExistingCredentialPolicy::Reject => ExistingHeaderBehavior::Reject,
        };
        if existing_credential && matches!(resolution, CredentialPlanResolution::Resolved { .. }) {
            return Err(AuthError::Configuration(
                crate::AuthConfigurationError::ExistingCredentialHeader,
            ));
        }
        let (authenticator, source) = match (resolution, existing_credential, self.placement) {
            (CredentialPlanResolution::Resolved { credential, source }, _, Placement::Bearer) => (
                HeaderAuth::bearer(credential.secret(), existing_behavior)?,
                Some(source),
            ),
            (
                CredentialPlanResolution::Resolved { credential, source },
                _,
                Placement::Header(name),
            ) => (
                HeaderAuth::new(
                    [(header_name(name)?, credential.secret().clone())],
                    existing_behavior,
                )?,
                Some(source),
            ),
            (CredentialPlanResolution::Unavailable, true, _)
                if self.existing_credential == ExistingCredentialPolicy::Accept =>
            {
                (HeaderAuth::new([], ExistingHeaderBehavior::Preserve)?, None)
            }
            (CredentialPlanResolution::Unavailable, _, _) => {
                return Err(AuthError::MissingApiKey {
                    provider: self.provider,
                });
            }
        };
        Ok(ResolvedAuth {
            authenticator,
            headers,
            context: ApiKeyAuthContext { source },
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

    #[derive(Debug)]
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
    async fn existing_bearer_header_requires_explicit_acceptance() {
        let context = Context {
            key: None,
            headers: vec![("Authorization".into(), "Bearer caller".into())],
        };
        let result = ApiKeyAuth::bearer("Provider", CredentialPlan::None)
            .resolve(&(), &context)
            .await;

        assert!(matches!(result, Err(AuthError::MissingApiKey { .. })));
    }

    #[tokio::test]
    async fn opted_in_existing_bearer_header_is_preserved() {
        let context = Context {
            key: None,
            headers: vec![("Authorization".into(), "Bearer caller".into())],
        };
        let resolved = ApiKeyAuth::bearer("Provider", CredentialPlan::None)
            .existing_credential_policy(ExistingCredentialPolicy::Accept)
            .resolve(&(), &context)
            .await
            .expect("accepted existing header resolves");
        assert_eq!(resolved.context, ApiKeyAuthContext { source: None });
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
        let result = ApiKeyAuth::bearer("Mistral", CredentialPlan::None)
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

    #[tokio::test]
    async fn resolved_plan_source_is_retained() {
        let resolved = ApiKeyAuth::bearer(
            "Provider",
            CredentialPlan::Resolved {
                credential: crate::ResolvedCredential::Static(SecretValue::new("key")),
                source: crate::CredentialSource::Environment,
            },
        )
        .resolve(
            &(),
            &Context {
                key: None,
                headers: Vec::new(),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            resolved.context.source,
            Some(crate::CredentialSource::Environment)
        );
    }

    #[tokio::test]
    async fn accepted_existing_header_conflicts_with_resolved_plan() {
        let result = ApiKeyAuth::bearer(
            "Provider",
            CredentialPlan::Resolved {
                credential: crate::ResolvedCredential::Static(SecretValue::new("planned")),
                source: crate::CredentialSource::Environment,
            },
        )
        .existing_credential_policy(ExistingCredentialPolicy::Accept)
        .resolve(
            &(),
            &Context {
                key: None,
                headers: vec![("Authorization".into(), "Bearer existing".into())],
            },
        )
        .await;

        assert!(matches!(
            result,
            Err(AuthError::Configuration(
                crate::AuthConfigurationError::ExistingCredentialHeader
            ))
        ));
    }
}
