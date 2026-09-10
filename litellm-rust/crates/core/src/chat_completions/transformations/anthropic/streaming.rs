use serde_json::Value;

use super::AnthropicMessagesTransformation;
use crate::chat_completions::response_utils::{finish_reason_for, usage_from_parts};
use crate::chat_completions::types::{
    ChatCompletionsOperation, ChatCompletionsStreamEvent, ChatCompletionsUsage,
};
use crate::error::Error;
use crate::operation::StreamTransformation;

pub(crate) struct AnthropicMessagesStreamInput {
    pub(crate) event: String,
    pub(crate) data: Value,
}

#[derive(Default)]
pub(crate) struct AnthropicMessagesStreamState {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_input_tokens: u64,
    cache_creation_input_tokens: u64,
}

impl AnthropicMessagesStreamState {
    fn update_usage(&mut self, usage: &Value) -> ChatCompletionsUsage {
        self.input_tokens = number(usage, "input_tokens").unwrap_or(self.input_tokens);
        self.output_tokens = number(usage, "output_tokens").unwrap_or(self.output_tokens);
        self.cache_read_input_tokens =
            number(usage, "cache_read_input_tokens").unwrap_or(self.cache_read_input_tokens);
        self.cache_creation_input_tokens = number(usage, "cache_creation_input_tokens")
            .unwrap_or(self.cache_creation_input_tokens);
        usage_from_parts(
            self.input_tokens,
            self.output_tokens,
            self.cache_read_input_tokens,
            self.cache_creation_input_tokens,
        )
    }
}

impl StreamTransformation<ChatCompletionsOperation> for AnthropicMessagesTransformation {
    type State = AnthropicMessagesStreamState;
    type Input = AnthropicMessagesStreamInput;
    type Error = Error;

    fn transform_stream_event(
        &self,
        state: &mut Self::State,
        input: Self::Input,
    ) -> Result<Vec<ChatCompletionsStreamEvent>, Self::Error> {
        let events = match input.event.as_str() {
            "message_start" => input
                .data
                .pointer("/message/usage")
                .map(|usage| vec![usage_event(state, usage)])
                .unwrap_or_default(),
            "content_block_start" => transform_block_start(&input.data)?,
            "content_block_delta" => transform_block_delta(&input.data)?,
            "message_delta" => transform_message_delta(state, &input.data),
            "error" => vec![ChatCompletionsStreamEvent::Error {
                message: input
                    .data
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Anthropic stream error")
                    .to_string(),
            }],
            "message_stop" | "content_block_stop" | "ping" => Vec::new(),
            _ => vec![ChatCompletionsStreamEvent::Unknown {
                event: input.event,
                data: input.data,
            }],
        };
        Ok(events)
    }
}

fn transform_block_start(data: &Value) -> Result<Vec<ChatCompletionsStreamEvent>, Error> {
    let index = number(data, "index").unwrap_or(0);
    let block = data
        .get("content_block")
        .ok_or(Error::MissingField("content_block"))?;
    match block.get("type").and_then(Value::as_str) {
        Some("tool_use") => Ok(vec![ChatCompletionsStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index: index,
            id: string(block, "id")?.to_string(),
            name: string(block, "name")?.to_string(),
        }]),
        Some("text") => Ok(block
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(|text| {
                vec![ChatCompletionsStreamEvent::TextDelta {
                    choice_index: 0,
                    text: text.to_string(),
                }]
            })
            .unwrap_or_default()),
        _ => Ok(Vec::new()),
    }
}

fn transform_block_delta(data: &Value) -> Result<Vec<ChatCompletionsStreamEvent>, Error> {
    let index = number(data, "index").unwrap_or(0);
    let delta = data.get("delta").ok_or(Error::MissingField("delta"))?;
    let event = match delta.get("type").and_then(Value::as_str) {
        Some("text_delta") => ChatCompletionsStreamEvent::TextDelta {
            choice_index: 0,
            text: string(delta, "text")?.to_string(),
        },
        Some("thinking_delta") => ChatCompletionsStreamEvent::ReasoningDelta {
            choice_index: 0,
            text: string(delta, "thinking")?.to_string(),
        },
        Some("signature_delta") => ChatCompletionsStreamEvent::ReasoningSignatureDelta {
            choice_index: 0,
            signature: string(delta, "signature")?.to_string(),
        },
        Some("input_json_delta") => ChatCompletionsStreamEvent::ToolCallDelta {
            choice_index: 0,
            tool_index: index,
            arguments: string(delta, "partial_json")?.to_string(),
        },
        _ => ChatCompletionsStreamEvent::Unknown {
            event: "content_block_delta".into(),
            data: data.clone(),
        },
    };
    Ok(vec![event])
}

fn transform_message_delta(
    state: &mut AnthropicMessagesStreamState,
    data: &Value,
) -> Vec<ChatCompletionsStreamEvent> {
    let finish = data
        .pointer("/delta/stop_reason")
        .and_then(Value::as_str)
        .map(|reason| ChatCompletionsStreamEvent::Finish {
            choice_index: 0,
            reason: finish_reason_for(reason).to_string(),
        });
    let usage = data.get("usage").map(|usage| usage_event(state, usage));
    finish.into_iter().chain(usage).collect()
}

fn usage_event(
    state: &mut AnthropicMessagesStreamState,
    usage: &Value,
) -> ChatCompletionsStreamEvent {
    ChatCompletionsStreamEvent::Usage {
        usage: state.update_usage(usage),
    }
}

fn number(value: &Value, field: &str) -> Option<u64> {
    value.get(field).and_then(Value::as_u64)
}

fn string<'a>(value: &'a Value, field: &'static str) -> Result<&'a str, Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or(Error::MissingField(field))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(
        state: &mut AnthropicMessagesStreamState,
        name: &str,
        data: Value,
    ) -> Vec<ChatCompletionsStreamEvent> {
        AnthropicMessagesTransformation
            .transform_stream_event(
                state,
                AnthropicMessagesStreamInput {
                    event: name.into(),
                    data,
                },
            )
            .unwrap()
    }

    #[test]
    fn preserves_text_tools_finish_and_split_usage() {
        let mut state = AnthropicMessagesStreamState::default();
        assert_eq!(
            event(
                &mut state,
                "message_start",
                json!({"message":{"usage":{"input_tokens":7,"cache_read_input_tokens":2}}})
            ),
            vec![ChatCompletionsStreamEvent::Usage {
                usage: usage_from_parts(7, 0, 2, 0)
            }]
        );
        assert_eq!(
            event(
                &mut state,
                "content_block_start",
                json!({"index":1,"content_block":{"type":"tool_use","id":"tool-1","name":"lookup"}})
            ),
            vec![ChatCompletionsStreamEvent::ToolCallStart {
                choice_index: 0,
                tool_index: 1,
                id: "tool-1".into(),
                name: "lookup".into(),
            }]
        );
        assert_eq!(
            event(
                &mut state,
                "content_block_delta",
                json!({"index":1,"delta":{"type":"input_json_delta","partial_json":"{\"q\":"}})
            ),
            vec![ChatCompletionsStreamEvent::ToolCallDelta {
                choice_index: 0,
                tool_index: 1,
                arguments: "{\"q\":".into(),
            }]
        );
        assert_eq!(
            event(
                &mut state,
                "message_delta",
                json!({"delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":4}})
            ),
            vec![
                ChatCompletionsStreamEvent::Finish {
                    choice_index: 0,
                    reason: "tool_calls".into(),
                },
                ChatCompletionsStreamEvent::Usage {
                    usage: usage_from_parts(7, 4, 2, 0),
                },
            ]
        );
    }

    #[test]
    fn preserves_unknown_events() {
        let mut state = AnthropicMessagesStreamState::default();
        assert_eq!(
            event(&mut state, "future_event", json!({"new":true})),
            vec![ChatCompletionsStreamEvent::Unknown {
                event: "future_event".into(),
                data: json!({"new":true}),
            }]
        );
    }
}
