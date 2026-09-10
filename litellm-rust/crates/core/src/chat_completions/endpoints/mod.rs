mod anthropic;
#[cfg(feature = "bedrock-auth")]
pub(crate) mod bedrock;

use crate::Error;

use super::registry::ChatCompletionsWireOperation;
use super::types::ResolvedChatCompletionsRequest;
use crate::operation::OperationPlan;

pub(crate) fn resolve_endpoint(
    request: &ResolvedChatCompletionsRequest<'_>,
) -> Result<String, Error> {
    match request.plan.wire_operation() {
        ChatCompletionsWireOperation::AnthropicMessages => anthropic::resolve(request),
        #[cfg(feature = "bedrock-auth")]
        ChatCompletionsWireOperation::BedrockConverse => bedrock::resolve(request),
    }
}
