//! The `/chat/completions` call, the Rust equivalent of Python's
//! `litellm.completion()`.
//!
//! [`chat_completions`] is the top-level entrypoint: give it a model, the
//! OpenAI-shaped message list, the provider-mapped optional params, and
//! credentials, and it resolves the provider, translates the conversation,
//! calls the provider, and returns a typed OpenAI-shaped response.

use crate::Error;
mod auth;
pub mod batch;
mod client;
pub mod conversation;
mod endpoints;
pub(crate) mod execution;
mod pipeline;
mod prepare;
mod registry;
pub mod response_utils;
mod streaming;
pub mod transformation;
mod transformations;
pub mod types;

use serde_json::{Map, Value};

use crate::operation::{CapabilitySupport, DeliveryMode, OperationPlan};

use pipeline::{perform_chat_completions_request, perform_chat_completions_stream};
use prepare::{parse_messages, resolve_request};
use registry::resolve_plan;
use transformations::transformation_for;
use types::{ChatCompletionsRequest, ChatCompletionsResponse, ChatCompletionsStream};

#[tracing::instrument(target = "litellm::function_trace", level = "trace", skip_all)]
pub async fn chat_completions(
    request: ChatCompletionsRequest<'_>,
) -> Result<ChatCompletionsResponse, Error> {
    if transformation::delivery_mode(&request.optional_params) == DeliveryMode::Stream {
        return Err(Error::InvalidRequest(
            "streaming chat completions require chat_completions_stream".into(),
        ));
    }
    perform_chat_completions_request(resolve_request(request)?).await
}

#[tracing::instrument(target = "litellm::function_trace", level = "trace", skip_all)]
pub async fn chat_completions_stream(
    mut request: ChatCompletionsRequest<'_>,
) -> Result<ChatCompletionsStream, Error> {
    request
        .optional_params
        .insert("stream".into(), Value::Bool(true));
    perform_chat_completions_stream(resolve_request(request)?).await
}

/// Whether the core would accept this request, without resolving credentials or
/// touching the network.
///
/// A host that keeps the Python implementation asks this first so it can emit
/// its pre-call logging exactly once, on whichever path is about to run.
/// Returns the decline reason, or `None` when the request is accepted.
pub fn chat_completions_decline_reason(
    model: &str,
    custom_llm_provider: Option<&str>,
    messages: Value,
    optional_params: &Map<String, Value>,
) -> Option<&'static str> {
    let Ok((_, plan)) = resolve_plan(model, custom_llm_provider) else {
        return Some("provider is not on the rust chat completions path");
    };
    let delivery = transformation::delivery_mode(optional_params);
    if plan.delivery_support(delivery) != CapabilitySupport::Supported {
        return Some(if delivery == DeliveryMode::Stream {
            "streaming"
        } else {
            "delivery mode"
        });
    }
    let Ok(messages) = parse_messages(messages) else {
        return Some("unreadable message list");
    };
    if messages.is_empty() {
        return Some("empty message list");
    }
    transformation_for(plan.transformation())
        .unsupported_reason(&messages, optional_params)
        .map(|reason| reason.0)
}

#[cfg(test)]
mod tests;
