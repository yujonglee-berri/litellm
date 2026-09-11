use crate::Error;
use crate::constants::ANTHROPIC_OAUTH_TOKEN_PREFIX;
use crate::http_utils::{has_header, string_headers};
use crate::providers::anthropic::messages::transformation::resolve_anthropic_api_key;

use super::registry::ChatCompletionsProvider;
use super::types::ResolvedChatCompletionsRequest;
use crate::operation::OperationPlan;
use litellm_auth::{AuthHandle, ExistingHeaderBehavior, HeaderAuth, NoAuth, SecretValue};

const HEADER_CONTEXT: &str = "chat completions";

pub(crate) fn authenticate(
    request: &ResolvedChatCompletionsRequest<'_>,
) -> Result<(Vec<(String, String)>, AuthHandle), Error> {
    let env_lookup = |key: &str| std::env::var(key).ok();
    authenticate_with_env(request, &env_lookup)
}

pub(crate) fn authenticate_with_env(
    request: &ResolvedChatCompletionsRequest<'_>,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<(Vec<(String, String)>, AuthHandle), Error> {
    let mut headers = string_headers(HEADER_CONTEXT, request.extra_headers.clone())?;
    let auth = match request.plan.provider() {
        ChatCompletionsProvider::Anthropic => {
            let auth = if defers_anthropic_auth(&headers) {
                AuthHandle::new(NoAuth)
            } else {
                let key = resolve_anthropic_api_key(request.api_key, env_lookup)?;
                headers.retain(|(name, _)| !name.eq_ignore_ascii_case("x-api-key"));
                headers.push(("x-api-key".into(), key.clone()));
                AuthHandle::new(HeaderAuth::new(
                    [(
                        reqwest::header::HeaderName::from_static("x-api-key"),
                        SecretValue::new(key),
                    )],
                    ExistingHeaderBehavior::Replace,
                )?)
            };
            add_default_headers(
                &mut headers,
                &[
                    ("anthropic-version", "2023-06-01"),
                    ("content-type", "application/json"),
                ],
            );
            auth
        }
        #[cfg(feature = "bedrock-auth")]
        ChatCompletionsProvider::Bedrock => {
            let auth = bedrock_auth(request, env_lookup);
            add_default_headers(&mut headers, &[("Content-Type", "application/json")]);
            auth?
        }
    };
    Ok((headers, auth))
}

fn add_default_headers(headers: &mut Vec<(String, String)>, defaults: &[(&str, &str)]) {
    for (name, value) in defaults {
        if !has_header(headers, name) {
            headers.push(((*name).into(), (*value).into()));
        }
    }
}

fn defers_anthropic_auth(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("authorization")
            && value
                .strip_prefix("Bearer ")
                .is_some_and(|token| token.starts_with(ANTHROPIC_OAUTH_TOKEN_PREFIX))
    })
}

#[cfg(feature = "bedrock-auth")]
fn bedrock_auth(
    request: &ResolvedChatCompletionsRequest<'_>,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<AuthHandle, Error> {
    use crate::providers::bedrock::aws_base::{
        AwsSigV4Auth, aws_auth_config, bedrock_model_id_and_region, host_supplied_credentials,
        resolve_bedrock_region,
    };
    use crate::providers::bedrock::constants::{AWS_BEARER_TOKEN_BEDROCK, BEDROCK_SERVICE};

    let bearer = match request.api_key {
        Some(key) => Some(key.to_string()),
        None => env_lookup(AWS_BEARER_TOKEN_BEDROCK),
    }
    .filter(|token| !token.trim().is_empty());
    if let Some(token) = bearer {
        // The final-request authenticator is authoritative. Mirroring the value in the
        // prepared headers preserves callback parity until logging consumes typed auth metadata.
        return HeaderAuth::bearer(&SecretValue::new(token), ExistingHeaderBehavior::Replace)
            .map(AuthHandle::new)
            .map_err(Error::from);
    }
    let (_, model_region) = bedrock_model_id_and_region(&request.model);
    let region = resolve_bedrock_region(
        model_region.as_deref(),
        &request.optional_params,
        env_lookup,
    );
    Ok(AuthHandle::new(AwsSigV4Auth::new(
        BEDROCK_SERVICE,
        region,
        aws_auth_config(&request.optional_params, env_lookup),
        host_supplied_credentials(&request.optional_params),
    )))
}
