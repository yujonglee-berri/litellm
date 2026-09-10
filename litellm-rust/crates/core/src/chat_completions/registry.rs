use crate::Error;
use crate::operation::{
    CapabilitySupport, DeliveryMode, OperationPlan, Provider, TransformationKind, WireOperation,
};
use crate::routing_utils::provider::{CustomLlmProvider, get_custom_llm_provider};

use super::types::ChatCompletionsOperation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatCompletionsProvider {
    Anthropic,
    #[cfg(feature = "bedrock-auth")]
    Bedrock,
}

impl Provider for ChatCompletionsProvider {
    fn name(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            #[cfg(feature = "bedrock-auth")]
            Self::Bedrock => "bedrock",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatCompletionsWireOperation {
    AnthropicMessages,
    #[cfg(feature = "bedrock-auth")]
    BedrockConverse,
}

impl WireOperation for ChatCompletionsWireOperation {
    fn name(self) -> &'static str {
        match self {
            Self::AnthropicMessages => "anthropic.messages",
            #[cfg(feature = "bedrock-auth")]
            Self::BedrockConverse => "bedrock.converse",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatCompletionsTransformationKind {
    AnthropicMessages,
    #[cfg(feature = "bedrock-auth")]
    BedrockConverse,
}

impl TransformationKind for ChatCompletionsTransformationKind {
    type WireOperation = ChatCompletionsWireOperation;

    fn wire_operation(self) -> Self::WireOperation {
        match self {
            Self::AnthropicMessages => ChatCompletionsWireOperation::AnthropicMessages,
            #[cfg(feature = "bedrock-auth")]
            Self::BedrockConverse => ChatCompletionsWireOperation::BedrockConverse,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatCompletionsPlan {
    AnthropicMessages,
    #[cfg(feature = "bedrock-auth")]
    BedrockConverse,
}

impl OperationPlan for ChatCompletionsPlan {
    type Operation = ChatCompletionsOperation;
    type Provider = ChatCompletionsProvider;
    type WireOperation = ChatCompletionsWireOperation;
    type Transformation = ChatCompletionsTransformationKind;

    fn provider(self) -> Self::Provider {
        match self {
            Self::AnthropicMessages => ChatCompletionsProvider::Anthropic,
            #[cfg(feature = "bedrock-auth")]
            Self::BedrockConverse => ChatCompletionsProvider::Bedrock,
        }
    }

    fn transformation(self) -> Self::Transformation {
        match self {
            Self::AnthropicMessages => ChatCompletionsTransformationKind::AnthropicMessages,
            #[cfg(feature = "bedrock-auth")]
            Self::BedrockConverse => ChatCompletionsTransformationKind::BedrockConverse,
        }
    }

    fn delivery_support(self, _delivery: DeliveryMode) -> CapabilitySupport {
        CapabilitySupport::Supported
    }
}

pub(crate) fn resolve_plan(
    model: &str,
    custom_llm_provider: Option<&str>,
) -> Result<(String, ChatCompletionsPlan), Error> {
    let provider = get_custom_llm_provider(model, custom_llm_provider)
        .or_else(|| {
            custom_llm_provider.map(|provider| CustomLlmProvider {
                model,
                custom_llm_provider: provider,
            })
        })
        .ok_or_else(|| {
            Error::InvalidProvider(
                "unable to resolve custom_llm_provider for chat completions request".into(),
            )
        })?;
    let plan = match provider.custom_llm_provider {
        "anthropic" => ChatCompletionsPlan::AnthropicMessages,
        #[cfg(feature = "bedrock-auth")]
        "bedrock" => ChatCompletionsPlan::BedrockConverse,
        value => return Err(Error::InvalidProvider(value.into())),
    };
    Ok((provider.model.into(), plan))
}

#[cfg(test)]
mod tests {
    use crate::operation::{
        CapabilitySupport, DeliveryMode, Fidelity, OperationPlan, Provider, WireOperation,
    };

    use super::*;
    use crate::chat_completions::transformations::transformation_for;

    #[test]
    fn anthropic_plan_keeps_provider_wire_operation_and_transformation_distinct() {
        let plan = ChatCompletionsPlan::AnthropicMessages;

        assert_eq!(plan.provider().name(), "anthropic");
        assert_eq!(plan.wire_operation().name(), "anthropic.messages");
        assert_eq!(
            plan.transformation(),
            ChatCompletionsTransformationKind::AnthropicMessages
        );
        assert_eq!(
            transformation_for(plan.transformation()).wire_operation(),
            plan.wire_operation()
        );
        let transformation = transformation_for(plan.transformation());
        assert_eq!(transformation.fidelity(), Fidelity::Exact);
        assert_eq!(
            plan.delivery_support(DeliveryMode::Complete),
            CapabilitySupport::Supported
        );
        assert_eq!(
            plan.delivery_support(DeliveryMode::Stream),
            CapabilitySupport::Supported
        );
    }

    #[cfg(feature = "bedrock-auth")]
    #[test]
    fn bedrock_plan_names_the_converse_wire_operation() {
        let plan = ChatCompletionsPlan::BedrockConverse;

        assert_eq!(plan.provider().name(), "bedrock");
        assert_eq!(plan.wire_operation().name(), "bedrock.converse");
        assert_eq!(
            plan.transformation(),
            ChatCompletionsTransformationKind::BedrockConverse
        );
        assert_eq!(
            transformation_for(plan.transformation()).wire_operation(),
            plan.wire_operation()
        );
    }
}
