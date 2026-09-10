use serde_json::{Map, Value};

use super::types::{ChatMessage, ChatMessageContent};
use crate::operation::DeliveryMode;

/// Why a request cannot be served by the Rust path.
///
/// The core declines rather than guessing: the host turns this into a
/// transparent fallback to the Python implementation, which covers the full
/// surface. Acceptance is an allowlist, so a parameter or message shape the
/// core has never seen declines by construction instead of being translated
/// wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unsupported(pub &'static str);

pub const STREAM_PARAM: &str = "stream";

pub fn delivery_mode(optional_params: &Map<String, Value>) -> DeliveryMode {
    if optional_params
        .get(STREAM_PARAM)
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        DeliveryMode::Stream
    } else {
        DeliveryMode::Complete
    }
}

/// Message fields that carry no meaning for the upstream body, so their
/// presence does not make a request untranslatable.
const IGNORABLE_MESSAGE_FIELDS: &[&str] = &["name"];

pub fn unsupported_param(
    supported: &'static [&'static str],
    config: &'static [&'static str],
    optional_params: &Map<String, Value>,
) -> Option<Unsupported> {
    optional_params
        .keys()
        .any(|key| {
            key != STREAM_PARAM
                && !supported.contains(&key.as_str())
                && !config.contains(&key.as_str())
        })
        .then_some(Unsupported("unrecognized request parameter"))
}

/// Message shapes the core can translate faithfully: text content, either a
/// plain string or a non-empty list of parts that are all
/// `{"type": "text", "text": ...}`. Tool calls, tool results, and multimodal
/// parts decline so Python's fuller translation handles them.
pub fn unsupported_message(message: &ChatMessage) -> Option<Unsupported> {
    if message
        .extra
        .keys()
        .any(|key| !IGNORABLE_MESSAGE_FIELDS.contains(&key.as_str()))
    {
        return Some(Unsupported("unrecognized message field"));
    }
    if !matches!(message.role.as_str(), "system" | "user" | "assistant") {
        return Some(Unsupported("unrecognized message role"));
    }
    match &message.content {
        None => Some(Unsupported("message without content")),
        Some(ChatMessageContent::Text(_)) => None,
        Some(ChatMessageContent::Parts(parts)) if parts.is_empty() => {
            Some(Unsupported("message without content"))
        }
        Some(ChatMessageContent::Parts(parts)) => parts
            .iter()
            .any(|part| {
                part.get("type").and_then(Value::as_str) != Some("text")
                    || part.get("text").and_then(Value::as_str).is_none()
                    || part.as_object().is_some_and(|object| object.len() != 2)
            })
            .then_some(Unsupported("non-text message content")),
    }
}
