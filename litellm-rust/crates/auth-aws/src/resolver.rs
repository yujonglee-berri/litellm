use aws_credential_types::Credentials;
use litellm_auth::{
    AuthError, AuthHandle, AuthResolver, ExistingHeaderBehavior, HeaderAuth, ResolvedAuth,
    SecretValue,
};

use crate::constants::{
    AWS_BEARER_TOKEN_BEDROCK, AWS_REGION, AWS_REGION_NAME, BEDROCK_SERVICE, DEFAULT_BEDROCK_REGION,
};
use crate::{AwsAuthConfig, AwsSigV4Auth, bedrock_model_id_and_region};

#[derive(Clone, Debug)]
pub struct BedrockAuthInput {
    pub model: String,
    pub api_key: Option<SecretValue>,
    pub region_name: Option<String>,
    pub aws_config: AwsAuthConfig,
    pub credentials: Option<Credentials>,
}

pub trait BedrockAuthSource<Call: ?Sized>: Sync {
    fn bedrock_auth_input(&self, call: &Call) -> BedrockAuthInput;

    fn env_lookup(&self, key: &str) -> Option<String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BedrockAuthKind {
    Bearer,
    SigV4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BedrockAuthContext {
    pub kind: BedrockAuthKind,
    pub region: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BedrockAuthResolver;

impl<Call, Services> AuthResolver<Call, Services> for BedrockAuthResolver
where
    Call: Sync + ?Sized,
    Services: BedrockAuthSource<Call> + ?Sized,
{
    type Authenticator = AuthHandle;
    type AuthContext = BedrockAuthContext;
    type Error = AuthError;

    async fn resolve(
        &self,
        call: &Call,
        services: &Services,
    ) -> Result<ResolvedAuth<AuthHandle, BedrockAuthContext>, AuthError> {
        let input = services.bedrock_auth_input(call);
        let (_, model_region) = bedrock_model_id_and_region(&input.model);
        let region = input
            .region_name
            .clone()
            .or(model_region)
            .or_else(|| services.env_lookup(AWS_REGION_NAME))
            .or_else(|| services.env_lookup(AWS_REGION))
            .unwrap_or_else(|| DEFAULT_BEDROCK_REGION.to_string());
        let bearer = input
            .api_key
            .as_ref()
            .map(|value| value.expose().to_string())
            .or_else(|| services.env_lookup(AWS_BEARER_TOKEN_BEDROCK))
            .filter(|token| !token.trim().is_empty());
        if let Some(token) = bearer {
            let authenticator =
                HeaderAuth::bearer(&SecretValue::new(token), ExistingHeaderBehavior::Replace)?;
            return Ok(ResolvedAuth {
                authenticator: AuthHandle::new(authenticator),
                headers: Vec::new(),
                context: BedrockAuthContext {
                    kind: BedrockAuthKind::Bearer,
                    region: Some(region),
                },
            });
        }

        let authenticator = AwsSigV4Auth::new(
            BEDROCK_SERVICE,
            region.clone(),
            input.aws_config.clone(),
            input.credentials.clone(),
        );
        Ok(ResolvedAuth {
            authenticator: AuthHandle::new(authenticator),
            headers: Vec::new(),
            context: BedrockAuthContext {
                kind: BedrockAuthKind::SigV4,
                region: Some(region),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use litellm_auth::{Auth, AuthResolver, AuthScheme};
    use reqwest::{Method, Request, Url};

    use super::*;

    struct TestCall {
        deployment: &'static str,
    }

    struct TestServices {
        api_key: Option<&'static str>,
        environment_token: Option<&'static str>,
        credentials: Option<Credentials>,
    }

    impl BedrockAuthSource<TestCall> for TestServices {
        fn bedrock_auth_input(&self, call: &TestCall) -> BedrockAuthInput {
            assert_eq!(call.deployment, "primary");
            BedrockAuthInput {
                model: "bedrock/us-east-1/anthropic.claude-v2".to_string(),
                api_key: self.api_key.map(SecretValue::new),
                region_name: None,
                aws_config: AwsAuthConfig::default(),
                credentials: self.credentials.clone(),
            }
        }

        fn env_lookup(&self, key: &str) -> Option<String> {
            match key {
                AWS_BEARER_TOKEN_BEDROCK => self.environment_token.map(str::to_string),
                _ => None,
            }
        }
    }

    #[tokio::test]
    async fn explicit_api_key_selects_bearer_auth_over_environment_and_sigv4() {
        let call = TestCall {
            deployment: "primary",
        };
        let services = TestServices {
            api_key: Some("request-token"),
            environment_token: Some("environment-token"),
            credentials: None,
        };
        let resolved = BedrockAuthResolver
            .resolve(&call, &services)
            .await
            .expect("resolves bearer auth");
        let request = Request::new(
            Method::POST,
            Url::parse("https://bedrock-runtime.us-east-1.amazonaws.com/model/test/invoke")
                .expect("valid URL"),
        );
        let request = resolved
            .authenticator
            .authenticate(request)
            .await
            .expect("applies bearer auth");

        assert_eq!(resolved.context.kind, BedrockAuthKind::Bearer);
        assert_eq!(resolved.context.region.as_deref(), Some("us-east-1"));
        assert_eq!(resolved.authenticator.scheme(), AuthScheme::Bearer);
        assert_eq!(
            request
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer request-token")
        );
    }

    #[tokio::test]
    async fn missing_bearer_selects_sigv4_and_signs_without_network() {
        let credentials = Credentials::new(
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            None,
            None,
            "test",
        );
        let call = TestCall {
            deployment: "primary",
        };
        let services = TestServices {
            api_key: None,
            environment_token: None,
            credentials: Some(credentials),
        };
        let resolved = BedrockAuthResolver
            .resolve(&call, &services)
            .await
            .expect("resolves SigV4 auth");
        let mut request = Request::new(
            Method::POST,
            Url::parse("https://bedrock-runtime.us-east-1.amazonaws.com/model/test/invoke")
                .expect("valid URL"),
        );
        *request.body_mut() = Some(reqwest::Body::from("{}"));
        let request = resolved
            .authenticator
            .authenticate(request)
            .await
            .expect("applies SigV4 auth");

        assert_eq!(resolved.context.kind, BedrockAuthKind::SigV4);
        assert_eq!(resolved.context.region.as_deref(), Some("us-east-1"));
        assert_eq!(resolved.authenticator.scheme(), AuthScheme::AwsSigV4);
        assert!(
            request
                .headers()
                .contains_key(reqwest::header::AUTHORIZATION)
        );
        assert!(request.headers().contains_key("x-amz-date"));
        assert!(
            request
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.contains("/us-east-1/bedrock/aws4_request"))
        );
    }
}
