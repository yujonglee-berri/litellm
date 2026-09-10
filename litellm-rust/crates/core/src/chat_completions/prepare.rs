use serde_json::Value;

use super::auth::authenticate;
use super::endpoints::resolve_endpoint;
use super::registry::resolve_plan;
use super::transformations::transformation_for;
use super::types::{
    ChatCompletionsInput, ChatCompletionsRequest, ChatMessage, ProviderChatCompletionsRequest,
    ResolvedChatCompletionsRequest,
};
use crate::error::Error;
use crate::operation::{CapabilitySupport, DeliveryMode, OperationPlan};

pub(super) fn parse_messages(messages: Value) -> Result<Vec<ChatMessage>, Error> {
    serde_json::from_value(messages)
        .map_err(|err| Error::InvalidRequest(format!("invalid chat completions messages: {err}")))
}

pub(super) fn resolve_request(
    request: ChatCompletionsRequest<'_>,
) -> Result<ResolvedChatCompletionsRequest<'_>, Error> {
    let (model, plan) = resolve_plan(request.model, request.custom_llm_provider)?;
    let delivery = super::transformation::delivery_mode(&request.optional_params);
    if plan.delivery_support(delivery) != CapabilitySupport::Supported {
        return Err(Error::Unsupported(if delivery == DeliveryMode::Stream {
            "streaming"
        } else {
            "delivery mode"
        }));
    }
    let messages = parse_messages(request.messages)?;
    if messages.is_empty() {
        return Err(Error::InvalidRequest(
            "chat completions requires at least one message".to_string(),
        ));
    }
    let transformation = transformation_for(plan.transformation());
    if transformation.wire_operation() != plan.wire_operation() {
        return Err(Error::InvalidProvider(
            "chat completions transformation does not match the resolved wire operation".into(),
        ));
    }
    if let Some(reason) = transformation.unsupported_reason(&messages, &request.optional_params) {
        return Err(Error::Unsupported(reason.0));
    }
    Ok(ResolvedChatCompletionsRequest {
        model,
        plan,
        messages,
        optional_params: request.optional_params,
        api_key: request.api_key,
        api_base: request.api_base,
        extra_headers: request.extra_headers,
        timeout: request.timeout,
    })
}

pub(super) fn prepare_provider_request(
    request: ResolvedChatCompletionsRequest<'_>,
) -> Result<ProviderChatCompletionsRequest, Error> {
    let (headers, auth) = authenticate(&request)?;
    let url = resolve_endpoint(&request)?;
    let model = request.model;
    let plan = request.plan;
    let transformed =
        transformation_for(plan.transformation()).transform_request(ChatCompletionsInput {
            model: model.clone(),
            messages: request.messages,
            optional_params: request.optional_params.clone(),
        })?;

    Ok(ProviderChatCompletionsRequest {
        model,
        plan,
        url,
        body: transformed.body,
        upstream_headers: headers,
        auth,
        timeout: request.timeout,
    })
}
