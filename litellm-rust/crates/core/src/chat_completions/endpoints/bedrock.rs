use serde_json::Value;

use crate::Error;
use crate::providers::bedrock::aws_base::{bedrock_model_id_and_region, resolve_bedrock_region};
use crate::providers::bedrock::constants::BEDROCK_RUNTIME_ENDPOINT_TEMPLATE;

use super::super::types::ResolvedChatCompletionsRequest;

const AWS_BEDROCK_RUNTIME_ENDPOINT: &str = "aws_bedrock_runtime_endpoint";
const CONVERSE_PATH_SUFFIX: &str = "/converse";
const CONVERSE_STREAM_PATH_SUFFIX: &str = "/converse-stream";

pub(super) fn resolve(request: &ResolvedChatCompletionsRequest<'_>) -> Result<String, Error> {
    let env_lookup = |key: &str| std::env::var(key).ok();
    resolve_with_env(request, &env_lookup)
}

pub(crate) fn resolve_with_env(
    request: &ResolvedChatCompletionsRequest<'_>,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<String, Error> {
    let (model_id, model_region) = bedrock_model_id_and_region(&request.model);
    let region = resolve_bedrock_region(
        model_region.as_deref(),
        &request.optional_params,
        env_lookup,
    );
    let stream = super::super::transformation::delivery_mode(&request.optional_params)
        == crate::operation::DeliveryMode::Stream;
    let suffix = if stream {
        CONVERSE_STREAM_PATH_SUFFIX
    } else {
        CONVERSE_PATH_SUFFIX
    };
    let endpoint = request
        .optional_params
        .get(AWS_BEDROCK_RUNTIME_ENDPOINT)
        .and_then(Value::as_str)
        .or(request.api_base)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| BEDROCK_RUNTIME_ENDPOINT_TEMPLATE.replace("{region}", &region));
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with(suffix) {
        return Ok(endpoint.into());
    }
    if stream && endpoint.ends_with(CONVERSE_PATH_SUFFIX) {
        return Ok(format!(
            "{}{CONVERSE_STREAM_PATH_SUFFIX}",
            endpoint.trim_end_matches(CONVERSE_PATH_SUFFIX)
        ));
    }
    Ok(format!("{endpoint}/model/{model_id}{suffix}"))
}
