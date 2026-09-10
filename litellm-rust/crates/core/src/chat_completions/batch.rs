use crate::operation::batch::{Batch, Cancel, Results, Retrieve, Submit};

use super::types::ChatCompletionsOperation;

pub type ChatCompletionsBatch = Batch<ChatCompletionsOperation>;
pub type SubmitChatCompletionsBatch = Batch<ChatCompletionsOperation, Submit>;
pub type RetrieveChatCompletionsBatch = Batch<ChatCompletionsOperation, Retrieve>;
pub type CancelChatCompletionsBatch = Batch<ChatCompletionsOperation, Cancel>;
pub type ChatCompletionsBatchResults = Batch<ChatCompletionsOperation, Results>;

#[cfg(test)]
mod tests {
    use serde_json::{Map, json};

    use super::*;
    use crate::chat_completions::types::{
        ChatCompletionsChoice, ChatCompletionsChoiceMessage, ChatCompletionsInput,
        ChatCompletionsResponse, ChatCompletionsUsage, ChatMessage, ChatMessageContent,
        PromptTokensDetails,
    };
    use crate::operation::Operation;
    use crate::operation::batch::{
        BatchItem, BatchItemError, BatchItemOutcome, BatchItemResult, BatchResultsResponse,
        BatchSubmitRequest,
    };

    #[test]
    fn submit_batch_contains_canonical_chat_completion_items() {
        let request: <ChatCompletionsBatch as Operation>::Request<'static> = BatchSubmitRequest {
            items: vec![BatchItem {
                custom_id: "request-1".into(),
                input: ChatCompletionsInput {
                    model: "model".into(),
                    messages: vec![ChatMessage {
                        role: "user".into(),
                        content: Some(ChatMessageContent::Text("hello".into())),
                        name: None,
                        extra: Map::new(),
                    }],
                    optional_params: Map::new(),
                },
            }],
        };

        assert_eq!(request.items[0].custom_id, "request-1");
        assert_eq!(request.items[0].input.model, "model");
    }

    #[test]
    fn results_preserve_successes_and_failures_per_item() {
        let response = ChatCompletionsResponse {
            created: 1,
            model: "model".into(),
            choices: vec![ChatCompletionsChoice {
                index: 0,
                message: ChatCompletionsChoiceMessage {
                    role: "assistant".into(),
                    content: Some("hello".into()),
                },
                finish_reason: "stop".into(),
            }],
            usage: ChatCompletionsUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
                prompt_tokens_details: PromptTokensDetails {
                    cached_tokens: 0,
                    cache_creation_tokens: 0,
                    text_tokens: 1,
                },
            },
        };
        let results: <ChatCompletionsBatchResults as Operation>::Response = BatchResultsResponse {
            items: vec![
                BatchItemResult {
                    custom_id: "ok".into(),
                    outcome: BatchItemOutcome::Succeeded { response },
                },
                BatchItemResult {
                    custom_id: "failed".into(),
                    outcome: BatchItemOutcome::Failed {
                        error: BatchItemError {
                            code: Some("invalid_request".into()),
                            message: "bad input".into(),
                        },
                    },
                },
            ],
            next_cursor: None,
        };

        assert_eq!(
            serde_json::to_value(results).unwrap(),
            json!({
                "items": [
                    {
                        "custom_id": "ok",
                        "outcome": {
                            "type": "succeeded",
                            "response": {
                                "created": 1,
                                "model": "model",
                                "choices": [{
                                    "index": 0,
                                    "message": {"role": "assistant", "content": "hello"},
                                    "finish_reason": "stop"
                                }],
                                "usage": {
                                    "prompt_tokens": 1,
                                    "completion_tokens": 1,
                                    "total_tokens": 2,
                                    "prompt_tokens_details": {
                                        "cached_tokens": 0,
                                        "cache_creation_tokens": 0,
                                        "text_tokens": 1
                                    }
                                }
                            }
                        }
                    },
                    {
                        "custom_id": "failed",
                        "outcome": {
                            "type": "failed",
                            "error": {"code": "invalid_request", "message": "bad input"}
                        }
                    }
                ],
                "next_cursor": null
            })
        );
    }
}
