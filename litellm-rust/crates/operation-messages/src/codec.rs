use litellm_operation::{Delivery, Error, OperationCodec, OperationStreamCodec};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{CallMessage, MessagesCall, MessagesResponse, MessagesStreamEvent};

const DEFAULT_MAX_TOKENS: u64 = 4096;
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

fn u64_parameter(call: &MessagesCall, name: &'static str) -> Result<Option<u64>, Error> {
    match call.parameters.get(name) {
        None => Ok(None),
        Some(Value::Number(number)) => number.as_u64().map(Some).ok_or_else(|| {
            Error::InvalidRequest(format!("messages parameter {name} must be an integer"))
        }),
        Some(_) => Err(Error::InvalidRequest(format!(
            "messages parameter {name} must be an integer"
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnthropicMessagesWire {
    pub version: String,
}

impl AnthropicMessagesWire {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
        }
    }
}

impl Default for AnthropicMessagesWire {
    fn default() -> Self {
        Self::new(DEFAULT_ANTHROPIC_VERSION)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnthropicMessagesParams {
    pub max_tokens: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub max_tokens: u64,
    pub messages: Vec<AnthropicMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicMessagesResponse {
    pub content: Vec<AnthropicContentBlock>,
    pub stop_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<AnthropicStreamDelta>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicStreamDelta {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

fn wire_messages(messages: &[CallMessage]) -> Vec<AnthropicMessage> {
    messages
        .iter()
        .map(|message| AnthropicMessage {
            role: message.role.clone(),
            content: message.content.clone(),
        })
        .collect()
}

impl OperationCodec for AnthropicMessagesWire {
    type Call = MessagesCall;
    type Context = ();
    type Params = AnthropicMessagesParams;
    type WireRequest = AnthropicMessagesRequest;
    type WireResponse = AnthropicMessagesResponse;
    type Response = MessagesResponse;

    fn params(&self, call: &MessagesCall) -> Result<AnthropicMessagesParams, Error> {
        Ok(AnthropicMessagesParams {
            max_tokens: u64_parameter(call, "max_tokens")?,
        })
    }

    fn encode(
        &self,
        call: &MessagesCall,
        params: &AnthropicMessagesParams,
        delivery: Delivery,
    ) -> Result<AnthropicMessagesRequest, Error> {
        if call.messages.is_empty() {
            return Err(Error::InvalidRequest(
                "messages requires at least one message".into(),
            ));
        }
        Ok(AnthropicMessagesRequest {
            model: call.model.clone(),
            max_tokens: params.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages: wire_messages(&call.messages),
            stream: delivery == Delivery::Stream,
            system: call.system.clone(),
        })
    }

    fn decode(
        &self,
        _call: &MessagesCall,
        response: AnthropicMessagesResponse,
    ) -> Result<MessagesResponse, Error> {
        Ok(MessagesResponse {
            content: response
                .content
                .into_iter()
                .filter(|block| block.kind == "text")
                .filter_map(|block| block.text)
                .collect::<Vec<_>>()
                .join(""),
            stop_reason: response.stop_reason,
        })
    }

    fn protocol_headers(&self) -> Vec<(String, String)> {
        vec![
            ("content-type".into(), "application/json".into()),
            ("anthropic-version".into(), self.version.clone()),
        ]
    }
}

impl OperationStreamCodec for AnthropicMessagesWire {
    type ServerEvent = AnthropicStreamEvent;
    type StreamEvent = MessagesStreamEvent;

    fn decode_event(
        &self,
        _call: &MessagesCall,
        event: AnthropicStreamEvent,
    ) -> Result<Option<MessagesStreamEvent>, Error> {
        Ok(match event.kind.as_str() {
            "content_block_delta" => event
                .delta
                .and_then(|delta| {
                    (delta.kind.as_deref() == Some("text_delta"))
                        .then_some(delta.text)
                        .flatten()
                })
                .map(MessagesStreamEvent::ContentDelta),
            "message_stop" => Some(MessagesStreamEvent::Finished),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn call(parameters: serde_json::Map<String, Value>) -> MessagesCall {
        MessagesCall {
            model: "claude-test".into(),
            messages: vec![CallMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
            system: None,
            parameters,
        }
    }

    #[test]
    fn params_read_typed_caller_parameters() {
        let mut parameters = serde_json::Map::new();
        parameters.insert("max_tokens".into(), json!(128));
        let params = AnthropicMessagesWire::default()
            .params(&call(parameters))
            .unwrap();
        assert_eq!(params.max_tokens, Some(128));
    }

    #[test]
    fn params_do_not_read_stream_from_the_parameter_map() {
        let mut parameters = serde_json::Map::new();
        parameters.insert("stream".into(), json!(true));
        let params = AnthropicMessagesWire::default()
            .params(&call(parameters))
            .unwrap();
        assert_eq!(params, AnthropicMessagesParams::default());
    }

    #[test]
    fn params_reject_mistyped_values() {
        let mut parameters = serde_json::Map::new();
        parameters.insert("max_tokens".into(), json!("many"));
        assert!(
            AnthropicMessagesWire::default()
                .params(&call(parameters))
                .is_err()
        );
    }

    #[test]
    fn encode_copies_the_model_id_unchanged() {
        let mut messages = call(serde_json::Map::new());
        messages.model = "anthropic/claude-haiku-4.5".into();
        let request = AnthropicMessagesWire::default()
            .encode(
                &messages,
                &AnthropicMessagesParams::default(),
                Delivery::Complete,
            )
            .unwrap();
        assert_eq!(request.model, "anthropic/claude-haiku-4.5");
    }

    #[test]
    fn encode_sets_stream_from_delivery() {
        let mut parameters = serde_json::Map::new();
        parameters.insert("stream".into(), json!(false));
        let messages = call(parameters);
        let params = AnthropicMessagesParams::default();
        let wire = AnthropicMessagesWire::default();
        assert!(
            !wire
                .encode(&messages, &params, Delivery::Complete)
                .unwrap()
                .stream
        );
        assert!(
            wire.encode(&messages, &params, Delivery::Stream)
                .unwrap()
                .stream
        );
    }

    #[test]
    fn encode_applies_the_anthropic_defaults() {
        let request = AnthropicMessagesWire::default()
            .encode(
                &call(serde_json::Map::new()),
                &AnthropicMessagesParams::default(),
                Delivery::Complete,
            )
            .unwrap();
        assert_eq!(request.max_tokens, DEFAULT_MAX_TOKENS);
        assert!(!request.stream);
        assert_eq!(request.messages.len(), 1);
    }

    #[test]
    fn encode_requires_a_message() {
        let mut empty = call(serde_json::Map::new());
        empty.messages.clear();
        assert!(
            AnthropicMessagesWire::default()
                .encode(
                    &empty,
                    &AnthropicMessagesParams::default(),
                    Delivery::Complete
                )
                .is_err()
        );
    }

    #[test]
    fn protocol_headers_include_version_and_content_type() {
        let headers = AnthropicMessagesWire::new("2023-06-01").protocol_headers();
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "content-type" && value == "application/json")
        );
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "anthropic-version" && value == "2023-06-01")
        );
    }

    #[test]
    fn decode_concatenates_only_text_blocks() {
        let response = AnthropicMessagesResponse {
            content: vec![
                AnthropicContentBlock {
                    kind: "text".into(),
                    text: Some("hello ".into()),
                },
                AnthropicContentBlock {
                    kind: "tool_use".into(),
                    text: None,
                },
                AnthropicContentBlock {
                    kind: "text".into(),
                    text: Some("world".into()),
                },
            ],
            stop_reason: Some("end_turn".into()),
        };
        let decoded = AnthropicMessagesWire::default()
            .decode(&call(serde_json::Map::new()), response)
            .unwrap();
        assert_eq!(decoded.content, "hello world");
        assert_eq!(decoded.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn stream_decode_maps_text_deltas_and_completion() {
        let delta = |kind: &str, text: Option<&str>| AnthropicStreamEvent {
            kind: "content_block_delta".into(),
            delta: Some(AnthropicStreamDelta {
                kind: Some(kind.into()),
                text: text.map(str::to_string),
                stop_reason: None,
            }),
        };
        let ignored = AnthropicStreamEvent {
            kind: "message_delta".into(),
            delta: Some(AnthropicStreamDelta {
                kind: None,
                text: None,
                stop_reason: Some("end_turn".into()),
            }),
        };

        let decode = |event| {
            AnthropicMessagesWire::default()
                .decode_event(&call(serde_json::Map::new()), event)
                .unwrap()
        };

        assert_eq!(
            decode(delta("text_delta", Some("Hello"))),
            Some(MessagesStreamEvent::ContentDelta("Hello".into()))
        );
        assert_eq!(decode(delta("input_json_delta", Some("{"))), None);
        assert_eq!(decode(ignored), None);
        assert_eq!(
            decode(AnthropicStreamEvent {
                kind: "message_stop".into(),
                delta: None,
            }),
            Some(MessagesStreamEvent::Finished)
        );
    }
}
