use serde_json::Value;

use super::BedrockConverseTransformation;
use crate::chat_completions::response_utils::{finish_reason_for, usage_from_parts};
use crate::chat_completions::types::{ChatCompletionsOperation, ChatCompletionsStreamEvent};
use crate::error::Error;
use crate::operation::StreamTransformation;

pub(crate) struct BedrockConverseStreamInput {
    pub(crate) event: String,
    pub(crate) data: Value,
}

impl StreamTransformation<ChatCompletionsOperation> for BedrockConverseTransformation {
    type State = ();
    type Input = BedrockConverseStreamInput;
    type Error = Error;

    fn transform_stream_event(
        &self,
        _state: &mut Self::State,
        input: Self::Input,
    ) -> Result<Vec<ChatCompletionsStreamEvent>, Self::Error> {
        let events = match input.event.as_str() {
            "contentBlockStart" => transform_block_start(&input.data)?,
            "contentBlockDelta" => transform_block_delta(&input.data),
            "messageStop" => input
                .data
                .get("stopReason")
                .and_then(Value::as_str)
                .map(|reason| {
                    vec![ChatCompletionsStreamEvent::Finish {
                        choice_index: 0,
                        reason: finish_reason_for(reason).to_string(),
                    }]
                })
                .unwrap_or_default(),
            "metadata" => transform_usage(&input.data),
            name if name.ends_with("Exception") => vec![ChatCompletionsStreamEvent::Error {
                message: input
                    .data
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Bedrock stream error")
                    .to_string(),
            }],
            "messageStart" | "contentBlockStop" => Vec::new(),
            _ => vec![ChatCompletionsStreamEvent::Unknown {
                event: input.event,
                data: input.data,
            }],
        };
        Ok(events)
    }
}

fn transform_block_start(data: &Value) -> Result<Vec<ChatCompletionsStreamEvent>, Error> {
    let Some(tool) = data.pointer("/start/toolUse") else {
        return Ok(Vec::new());
    };
    Ok(vec![ChatCompletionsStreamEvent::ToolCallStart {
        choice_index: 0,
        tool_index: number(data, "contentBlockIndex").unwrap_or(0),
        id: string(tool, "toolUseId")?.to_string(),
        name: string(tool, "name")?.to_string(),
    }])
}

fn transform_block_delta(data: &Value) -> Vec<ChatCompletionsStreamEvent> {
    let index = number(data, "contentBlockIndex").unwrap_or(0);
    let Some(delta) = data.get("delta") else {
        return Vec::new();
    };
    if let Some(text) = delta.get("text").and_then(Value::as_str) {
        return vec![ChatCompletionsStreamEvent::TextDelta {
            choice_index: 0,
            text: text.to_string(),
        }];
    }
    if let Some(text) = delta
        .pointer("/reasoningContent/text")
        .and_then(Value::as_str)
    {
        return vec![ChatCompletionsStreamEvent::ReasoningDelta {
            choice_index: 0,
            text: text.to_string(),
        }];
    }
    if let Some(arguments) = delta.pointer("/toolUse/input").and_then(Value::as_str) {
        return vec![ChatCompletionsStreamEvent::ToolCallDelta {
            choice_index: 0,
            tool_index: index,
            arguments: arguments.to_string(),
        }];
    }
    vec![ChatCompletionsStreamEvent::Unknown {
        event: "contentBlockDelta".into(),
        data: data.clone(),
    }]
}

fn transform_usage(data: &Value) -> Vec<ChatCompletionsStreamEvent> {
    data.get("usage")
        .map(|usage| {
            vec![ChatCompletionsStreamEvent::Usage {
                usage: usage_from_parts(
                    number(usage, "inputTokens").unwrap_or(0),
                    number(usage, "outputTokens").unwrap_or(0),
                    number(usage, "cacheReadInputTokens").unwrap_or(0),
                    number(usage, "cacheWriteInputTokens").unwrap_or(0),
                ),
            }]
        })
        .unwrap_or_default()
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

    #[test]
    fn converts_text_finish_and_usage_events() {
        let mut state = ();
        let transform = |state: &mut (), event: &str, data: Value| {
            BedrockConverseTransformation
                .transform_stream_event(
                    state,
                    BedrockConverseStreamInput {
                        event: event.into(),
                        data,
                    },
                )
                .unwrap()
        };
        assert_eq!(
            transform(
                &mut state,
                "contentBlockDelta",
                json!({"contentBlockIndex":0,"delta":{"text":"hello"}})
            ),
            vec![ChatCompletionsStreamEvent::TextDelta {
                choice_index: 0,
                text: "hello".into(),
            }]
        );
        assert_eq!(
            transform(&mut state, "messageStop", json!({"stopReason":"end_turn"})),
            vec![ChatCompletionsStreamEvent::Finish {
                choice_index: 0,
                reason: "stop".into(),
            }]
        );
        assert_eq!(
            transform(
                &mut state,
                "metadata",
                json!({"usage":{"inputTokens":3,"outputTokens":2}})
            ),
            vec![ChatCompletionsStreamEvent::Usage {
                usage: usage_from_parts(3, 2, 0, 0),
            }]
        );
    }
}
