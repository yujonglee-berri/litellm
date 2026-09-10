use crate::Error;

use super::execution::execute_chat_completions_provider_call;
use super::prepare::prepare_provider_request;
use super::streaming::execute_chat_completions_provider_stream;
use super::types::{
    ChatCompletionsResponse, ChatCompletionsStream, ResolvedChatCompletionsRequest,
};

pub(super) async fn perform_chat_completions_request(
    request: ResolvedChatCompletionsRequest<'_>,
) -> Result<ChatCompletionsResponse, Error> {
    execute_chat_completions_provider_call(prepare_provider_request(request)?).await
}

pub(super) async fn perform_chat_completions_stream(
    request: ResolvedChatCompletionsRequest<'_>,
) -> Result<ChatCompletionsStream, Error> {
    execute_chat_completions_provider_stream(prepare_provider_request(request)?).await
}
