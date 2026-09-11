use std::pin::Pin;
use std::time::Duration;

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::registry::ChatCompletionsPlan;
use crate::operation::{Operation, OperationKind, StreamingOperation};
use litellm_auth::AuthHandle;

pub struct ChatCompletionsOperation;

pub type ChatCompletionsStream =
    Pin<Box<dyn Stream<Item = Result<ChatCompletionsStreamEvent, crate::Error>> + Send>>;

/// A `/chat/completions` call as it crosses into the core.
///
/// `optional_params` arrives already mapped to the provider's own parameter
/// names by the host, exactly as the messages route receives an already
/// Anthropic-shaped body. The core owns the conversation translation, the
/// provider call, and the response normalization.
pub struct ChatCompletionsRequest<'a> {
    pub model: &'a str,
    pub messages: Value,
    pub optional_params: Map<String, Value>,
    pub api_key: Option<&'a str>,
    pub api_base: Option<&'a str>,
    pub custom_llm_provider: Option<&'a str>,
    pub extra_headers: Option<Map<String, Value>>,
    pub timeout: Option<Duration>,
}

pub(super) struct ResolvedChatCompletionsRequest<'a> {
    pub(super) model: String,
    pub(super) plan: ChatCompletionsPlan,
    pub(super) messages: Vec<ChatMessage>,
    pub(super) optional_params: Map<String, Value>,
    pub(super) api_key: Option<&'a str>,
    pub(super) api_base: Option<&'a str>,
    pub(super) extra_headers: Option<Map<String, Value>>,
    pub(super) timeout: Option<Duration>,
}

pub(super) struct ProviderChatCompletionsRequest {
    pub(super) model: String,
    pub(super) plan: ChatCompletionsPlan,
    pub(super) url: String,
    pub(super) body: Value,
    pub(super) upstream_headers: Vec<(String, String)>,
    pub(super) auth: AuthHandle,
    pub(super) timeout: Option<Duration>,
}

impl Operation for ChatCompletionsOperation {
    type Request<'a> = ChatCompletionsInput;
    type Response = ChatCompletionsResponse;

    const KIND: OperationKind = OperationKind::ChatCompletions;
}

impl StreamingOperation for ChatCompletionsOperation {
    type StreamEvent = ChatCompletionsStreamEvent;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionsInput {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub optional_params: Map<String, Value>,
}

pub(crate) struct ChatCompletionsTransformResponse {
    #[cfg_attr(not(feature = "bedrock-auth"), allow(dead_code))]
    pub(crate) model: String,
    pub(crate) response: ProviderChatResponseData,
}

/// The provider-shaped request body a transformation produces. Named rather than a bare
/// `Value` so the transform contract stays a typed one, mirroring
/// [`crate::audio_transcription::types::AudioTranscriptionRequestData`].
pub struct ProviderChatRequestData {
    pub body: Value,
}

/// The raw provider response body handed to a transformation for normalization.
pub struct ProviderChatResponseData {
    pub body: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCompletionsStreamEvent {
    TextDelta {
        choice_index: u64,
        text: String,
    },
    ReasoningDelta {
        choice_index: u64,
        text: String,
    },
    ReasoningSignatureDelta {
        choice_index: u64,
        signature: String,
    },
    ToolCallStart {
        choice_index: u64,
        tool_index: u64,
        id: String,
        name: String,
    },
    ToolCallDelta {
        choice_index: u64,
        tool_index: u64,
        arguments: String,
    },
    Usage {
        usage: ChatCompletionsUsage,
    },
    Finish {
        choice_index: u64,
        reason: String,
    },
    Error {
        message: String,
    },
    Unknown {
        event: String,
        data: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatMessageContent {
    Text(String),
    Parts(Vec<Value>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ChatMessageContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// OpenAI `usage`, including the `prompt_tokens_details` split LiteLLM's Python
/// path reports so cost tracking sees the same numbers on either path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptTokensDetails {
    pub cached_tokens: u64,
    pub cache_creation_tokens: u64,
    pub text_tokens: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionsUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub prompt_tokens_details: PromptTokensDetails,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionsChoiceMessage {
    pub role: String,
    // Whether an empty turn is `None` or `""` is the provider's choice, not a
    // shared invariant: Anthropic's transform ends on `merged_text or None`
    // while Converse assigns the joined string unconditionally. Each config
    // mirrors its own, so keep this optional and serialize it even when None.
    pub content: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionsChoice {
    pub index: u64,
    pub message: ChatCompletionsChoiceMessage,
    pub finish_reason: String,
}

/// The normalized response handed back to the host.
///
/// There is deliberately no `id`: Python mints the `chatcmpl-…` id on the
/// `ModelResponse` it already created, and echoing the provider's own id here
/// would change it. Pinned by `response_carries_no_id` in `tests.rs`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionsResponse {
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatCompletionsChoice>,
    pub usage: ChatCompletionsUsage,
}
