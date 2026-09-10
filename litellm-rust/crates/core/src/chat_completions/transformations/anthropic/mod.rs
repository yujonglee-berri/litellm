use serde_json::{Map, Value, json};

pub(crate) mod streaming;

use crate::chat_completions::conversation::{Conversation, build_conversation};
use crate::chat_completions::transformation::{
    Unsupported, unsupported_message, unsupported_param,
};
use crate::chat_completions::transformations::ChatCompletionsTransformation;
use crate::chat_completions::types::{
    ChatCompletionsChoice, ChatCompletionsChoiceMessage, ChatCompletionsInput,
    ChatCompletionsResponse, ChatCompletionsTransformResponse, ChatMessage,
    ProviderChatRequestData,
};
use crate::error::Error;
use crate::operation::{
    Fidelity, OperationTransformation, RequestTransformation, ResponseTransformation,
};

use crate::chat_completions::registry::ChatCompletionsWireOperation;
use crate::chat_completions::response_utils::{finish_reason_for, unix_now, usage_from_parts};
use crate::chat_completions::types::ChatCompletionsOperation;

/// Anthropic parameter names, post `map_openai_params`, that the Rust path can
/// place verbatim in the Messages body.
///
/// `top_k` is deliberately absent even though the Messages API takes it.
/// `temperature` and `top_p` reach this gate already resolved, because
/// `map_openai_params` runs first and applies `_apply_sampling_param` to them.
/// `top_k` bypasses `map_openai_params` entirely, so Python applies that same
/// per-model gate inside `transform_request`, the function this route replaces.
/// Forwarding it would send `top_k` to a model that removed sampling params and
/// take a 400 after the call, where Python drops it and succeeds.
const ACCEPTED_MAPPED_PARAMS: &[&str] = &["max_tokens", "temperature", "top_p", "stop_sequences"];

pub(crate) struct AnthropicMessagesTransformation;

pub(crate) const ANTHROPIC_MESSAGES_TRANSFORMATION: AnthropicMessagesTransformation =
    AnthropicMessagesTransformation;

fn text_block(text: &str) -> Value {
    json!({"type": "text", "text": text})
}

fn anthropic_body(model: &str, conversation: &Conversation, params: Map<String, Value>) -> Value {
    let messages: Vec<Value> = conversation
        .turns
        .iter()
        .map(|turn| {
            json!({
                "role": turn.role.as_str(),
                "content": turn.texts.iter().map(|text| text_block(text)).collect::<Vec<_>>(),
            })
        })
        .collect();

    let system: Vec<Value> = conversation.system.iter().map(|s| text_block(s)).collect();

    let body = Map::from_iter(
        [
            ("model".to_string(), json!(model)),
            ("messages".to_string(), json!(messages)),
        ]
        .into_iter()
        // Python builds `{"model", "messages", **optional_params}` with
        // `system` already folded into optional_params, so a caller-supplied
        // key of the same name wins here too.
        .chain((!system.is_empty()).then(|| ("system".to_string(), json!(system))))
        .chain(params),
    );
    Value::Object(body)
}

impl OperationTransformation<ChatCompletionsOperation> for AnthropicMessagesTransformation {
    type WireOperation = ChatCompletionsWireOperation;

    fn wire_operation(&self) -> Self::WireOperation {
        ChatCompletionsWireOperation::AnthropicMessages
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Exact
    }
}

impl ChatCompletionsTransformation for AnthropicMessagesTransformation {
    #[tracing::instrument(target = "litellm::function_trace", level = "trace", skip_all)]
    fn accepted_mapped_params(&self) -> &'static [&'static str] {
        ACCEPTED_MAPPED_PARAMS
    }

    fn unsupported_reason(
        &self,
        messages: &[ChatMessage],
        optional_params: &Map<String, Value>,
    ) -> Option<Unsupported> {
        unsupported_param(self.accepted_mapped_params(), &[], optional_params)
            .or_else(|| messages.iter().find_map(unsupported_message))
            // Anthropic rejects a request whose first turn is not a user turn.
            // Python only repairs that under `litellm.modify_params`, which the
            // core cannot observe, so decline instead of guessing.
            .or_else(|| {
                (!build_conversation(messages).opens_on_user_turn())
                    .then_some(Unsupported("conversation does not open on a user turn"))
            })
    }
}

impl RequestTransformation<ChatCompletionsOperation> for AnthropicMessagesTransformation {
    type Input = ChatCompletionsInput;
    type Output = ProviderChatRequestData;
    type Error = Error;

    #[tracing::instrument(target = "litellm::function_trace", level = "trace", skip_all)]
    fn transform_request(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let ChatCompletionsInput {
            model,
            messages,
            optional_params,
        } = input;
        Ok(ProviderChatRequestData {
            body: anthropic_body(&model, &build_conversation(&messages), optional_params),
        })
    }
}

impl ResponseTransformation<ChatCompletionsOperation> for AnthropicMessagesTransformation {
    type Input = ChatCompletionsTransformResponse;
    type Error = Error;

    #[tracing::instrument(target = "litellm::function_trace", level = "trace", skip_all)]
    fn transform_response(
        &self,
        input: Self::Input,
    ) -> Result<ChatCompletionsResponse, Self::Error> {
        let ChatCompletionsTransformResponse { model: _, response } = input;
        let body = response
            .body
            .as_object()
            .ok_or_else(|| Error::InvalidResponse("messages response is not an object".into()))?;

        let content = body
            .get("content")
            .and_then(Value::as_array)
            .ok_or(Error::MissingField("content"))?;
        // The route declines tool and thinking requests, so a non-text block
        // means the response carries something this path never asked for.
        // Decline rather than silently dropping it; the host falls back.
        if content
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) != Some("text"))
        {
            return Err(Error::Unsupported("non-text response content block"));
        }
        let text: String = content
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect();

        let usage = body
            .get("usage")
            .and_then(Value::as_object)
            .ok_or(Error::MissingField("usage"))?;
        let field = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);

        Ok(ChatCompletionsResponse {
            created: unix_now(),
            model: body
                .get("model")
                .and_then(Value::as_str)
                .ok_or(Error::MissingField("model"))?
                .to_string(),
            choices: vec![ChatCompletionsChoice {
                index: 0,
                message: ChatCompletionsChoiceMessage {
                    role: "assistant".to_string(),
                    content: (!text.is_empty()).then_some(text),
                },
                finish_reason: finish_reason_for(
                    body.get("stop_reason")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                .to_string(),
            }],
            usage: usage_from_parts(
                field("input_tokens"),
                field("output_tokens"),
                field("cache_read_input_tokens"),
                field("cache_creation_input_tokens"),
            ),
        })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
