use litellm_operation::{
    Complete, Fidelity, OperationPlan, OperationTransformation, Provider, Stream, SupportsWire,
    TransformationForDelivery, TransformationKind, WireOperation,
};

use crate::ChatCompletions;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Anthropic;

impl Provider for Anthropic {
    const NAME: &'static str = "anthropic";
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Bedrock;

impl Provider for Bedrock {
    const NAME: &'static str = "bedrock";
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnthropicMessages;

impl WireOperation for AnthropicMessages {
    const NAME: &'static str = "anthropic.messages";
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BedrockConverse;

impl WireOperation for BedrockConverse {
    const NAME: &'static str = "bedrock.converse";
}

impl SupportsWire<AnthropicMessages, Complete> for Anthropic {}
impl SupportsWire<AnthropicMessages, Stream> for Anthropic {}
impl SupportsWire<BedrockConverse, Complete> for Bedrock {}
impl SupportsWire<BedrockConverse, Stream> for Bedrock {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnthropicMessagesTransformation;

impl OperationTransformation<ChatCompletions, AnthropicMessages>
    for AnthropicMessagesTransformation
{
    const FIDELITY: Fidelity = Fidelity::Exact;
}

impl TransformationKind<ChatCompletions, AnthropicMessages> for AnthropicMessagesTransformation {
    const NAME: &'static str = "chat_completions.to_anthropic_messages";
}

impl TransformationForDelivery<ChatCompletions, AnthropicMessages, Complete>
    for AnthropicMessagesTransformation
{
}

impl TransformationForDelivery<ChatCompletions, AnthropicMessages, Stream>
    for AnthropicMessagesTransformation
{
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BedrockConverseTransformation;

impl OperationTransformation<ChatCompletions, BedrockConverse> for BedrockConverseTransformation {
    const FIDELITY: Fidelity = Fidelity::Emulated;
}

impl TransformationKind<ChatCompletions, BedrockConverse> for BedrockConverseTransformation {
    const NAME: &'static str = "chat_completions.to_bedrock_converse";
}

impl TransformationForDelivery<ChatCompletions, BedrockConverse, Complete>
    for BedrockConverseTransformation
{
}

impl TransformationForDelivery<ChatCompletions, BedrockConverse, Stream>
    for BedrockConverseTransformation
{
}

pub type AnthropicCompletePlan = OperationPlan<
    ChatCompletions,
    Anthropic,
    AnthropicMessages,
    Complete,
    AnthropicMessagesTransformation,
>;
pub type AnthropicStreamPlan = OperationPlan<
    ChatCompletions,
    Anthropic,
    AnthropicMessages,
    Stream,
    AnthropicMessagesTransformation,
>;
pub type BedrockCompletePlan = OperationPlan<
    ChatCompletions,
    Bedrock,
    BedrockConverse,
    Complete,
    BedrockConverseTransformation,
>;
pub type BedrockStreamPlan =
    OperationPlan<ChatCompletions, Bedrock, BedrockConverse, Stream, BedrockConverseTransformation>;

pub const fn anthropic_complete() -> AnthropicCompletePlan {
    OperationPlan::new(
        Anthropic,
        AnthropicMessages,
        AnthropicMessagesTransformation,
    )
}

pub const fn anthropic_stream() -> AnthropicStreamPlan {
    OperationPlan::new(
        Anthropic,
        AnthropicMessages,
        AnthropicMessagesTransformation,
    )
}

pub const fn bedrock_complete() -> BedrockCompletePlan {
    OperationPlan::new(Bedrock, BedrockConverse, BedrockConverseTransformation)
}

pub const fn bedrock_stream() -> BedrockStreamPlan {
    OperationPlan::new(Bedrock, BedrockConverse, BedrockConverseTransformation)
}

#[cfg(test)]
mod tests {
    use litellm_operation::{Delivery, OperationTransformation, Provider, WireOperation};

    use super::*;

    #[test]
    fn plans_keep_provider_wire_semantics_and_delivery_distinct() {
        let complete = anthropic_complete();
        let stream = anthropic_stream();

        fn accepts_complete(_: AnthropicCompletePlan) {}
        fn accepts_stream(_: AnthropicStreamPlan) {}

        accepts_complete(complete);
        accepts_stream(stream);

        assert_eq!(Anthropic::NAME, "anthropic");
        assert_eq!(AnthropicMessages::NAME, "anthropic.messages");
        assert_eq!(complete.delivery(), Delivery::Complete);
        assert_eq!(stream.delivery(), Delivery::Stream);
        assert_eq!(AnthropicMessagesTransformation::FIDELITY, Fidelity::Exact);
    }

    #[test]
    fn bedrock_is_a_separate_provider_wire_pair() {
        let complete = bedrock_complete();
        let stream = bedrock_stream();

        fn accepts_complete(_: BedrockCompletePlan) {}
        fn accepts_stream(_: BedrockStreamPlan) {}

        accepts_complete(complete);
        accepts_stream(stream);

        assert_eq!(Bedrock::NAME, "bedrock");
        assert_eq!(BedrockConverse::NAME, "bedrock.converse");
        assert_eq!(complete.delivery(), Delivery::Complete);
        assert_eq!(stream.delivery(), Delivery::Stream);
        assert_eq!(BedrockConverseTransformation::FIDELITY, Fidelity::Emulated);
    }
}
