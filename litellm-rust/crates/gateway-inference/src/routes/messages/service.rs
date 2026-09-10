use std::sync::Arc;

use litellm_core::Error;
use litellm_core::messages::types::MessagesRequest;
use litellm_core::messages::{messages, messages_stream};
use litellm_gateway_router::Router;
use serde_json::{Map, Value};

pub(crate) enum MessagesResponse {
    Json(Value),
    Stream(reqwest::Response),
}

#[tracing::instrument(
    name = "messages_gateway_service",
    target = "litellm::function_trace",
    level = "trace",
    skip_all
)]
pub async fn run(
    router: &Arc<Router>,
    body: Value,
    extra_headers: Option<Map<String, Value>>,
) -> Result<MessagesResponse, Error> {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| Error::InvalidRequest("messages body requires a model".to_string()))?;
    let deployment = router
        .get_available_deployment(model)
        .ok_or_else(|| Error::Routing(format!("no deployment available for model '{model}'")))?;
    let provider_model = deployment.litellm_params.model.as_str();
    let request = MessagesRequest {
        model: provider_model,
        body,
        api_key: deployment.litellm_params.api_key.as_deref(),
        api_base: deployment.litellm_params.api_base.as_deref(),
        custom_llm_provider: None,
        extra_headers,
        timeout: None,
    };
    if request.body.get("stream").and_then(Value::as_bool) == Some(true) {
        return messages_stream(request).await.map(MessagesResponse::Stream);
    }

    let response = messages(request).await?;
    serde_json::to_value(response)
        .map(MessagesResponse::Json)
        .map_err(|err| {
            Error::InvalidResponse(format!("failed to serialize messages response: {err}"))
        })
}
