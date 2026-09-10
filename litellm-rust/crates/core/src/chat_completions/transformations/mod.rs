pub(crate) mod anthropic;
#[cfg(feature = "bedrock-auth")]
pub(crate) mod bedrock;

use serde_json::{Map, Value};

use crate::Error;
use crate::operation::{
    OperationTransformation, RequestTransformation, ResponseTransformation, StreamTransformation,
};

use super::registry::{ChatCompletionsTransformationKind, ChatCompletionsWireOperation};
use super::transformation::Unsupported;
use super::types::{
    ChatCompletionsInput, ChatCompletionsOperation, ChatCompletionsStreamEvent,
    ChatCompletionsTransformResponse, ChatMessage, ProviderChatRequestData,
};

pub(crate) enum ChatCompletionsStreamState {
    Anthropic(anthropic::streaming::AnthropicMessagesStreamState),
    #[cfg(feature = "bedrock-auth")]
    Bedrock,
}

impl ChatCompletionsStreamState {
    pub(crate) fn new(transformation: ChatCompletionsTransformationKind) -> Self {
        match transformation {
            ChatCompletionsTransformationKind::AnthropicMessages => {
                Self::Anthropic(Default::default())
            }
            #[cfg(feature = "bedrock-auth")]
            ChatCompletionsTransformationKind::BedrockConverse => Self::Bedrock,
        }
    }
}

pub(crate) fn transform_stream_event(
    transformation: ChatCompletionsTransformationKind,
    state: &mut ChatCompletionsStreamState,
    event: String,
    data: Value,
) -> Result<Vec<ChatCompletionsStreamEvent>, Error> {
    match (transformation, state) {
        (
            ChatCompletionsTransformationKind::AnthropicMessages,
            ChatCompletionsStreamState::Anthropic(state),
        ) => anthropic::ANTHROPIC_MESSAGES_TRANSFORMATION.transform_stream_event(
            state,
            anthropic::streaming::AnthropicMessagesStreamInput { event, data },
        ),
        #[cfg(feature = "bedrock-auth")]
        (
            ChatCompletionsTransformationKind::BedrockConverse,
            ChatCompletionsStreamState::Bedrock,
        ) => bedrock::BEDROCK_CONVERSE_TRANSFORMATION.transform_stream_event(
            &mut (),
            bedrock::streaming::BedrockConverseStreamInput { event, data },
        ),
        #[allow(unreachable_patterns)]
        _ => Err(Error::InvalidProvider(
            "stream state does not match chat completions transformation".into(),
        )),
    }
}

pub(crate) trait ChatCompletionsTransformation:
    OperationTransformation<ChatCompletionsOperation, WireOperation = ChatCompletionsWireOperation>
    + RequestTransformation<
        ChatCompletionsOperation,
        Input = ChatCompletionsInput,
        Output = ProviderChatRequestData,
        Error = Error,
    > + ResponseTransformation<
        ChatCompletionsOperation,
        Input = ChatCompletionsTransformResponse,
        Error = Error,
    > + Sync
{
    /// Provider-mapped body fields this transformation can translate with Python parity
    fn accepted_mapped_params(&self) -> &'static [&'static str];

    /// Parameters consumed as call configuration rather than serialized.
    fn config_params(&self) -> &'static [&'static str] {
        &[]
    }

    fn unsupported_reason(
        &self,
        messages: &[ChatMessage],
        optional_params: &Map<String, Value>,
    ) -> Option<Unsupported> {
        super::transformation::unsupported_param(
            self.accepted_mapped_params(),
            self.config_params(),
            optional_params,
        )
        .or_else(|| {
            messages
                .iter()
                .find_map(super::transformation::unsupported_message)
        })
    }
}

pub(crate) fn transformation_for(
    transformation: ChatCompletionsTransformationKind,
) -> &'static dyn ChatCompletionsTransformation {
    match transformation {
        ChatCompletionsTransformationKind::AnthropicMessages => {
            &anthropic::ANTHROPIC_MESSAGES_TRANSFORMATION
        }
        #[cfg(feature = "bedrock-auth")]
        ChatCompletionsTransformationKind::BedrockConverse => {
            &bedrock::BEDROCK_CONVERSE_TRANSFORMATION
        }
    }
}
