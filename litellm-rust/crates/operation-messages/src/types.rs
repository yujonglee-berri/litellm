use litellm_operation::{Operation, StreamingOperation};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Messages;

impl Operation for Messages {
    type Request<'a> = MessagesRequest<'a>;
    type Response = MessagesResponse;

    const NAME: &'static str = "messages";
}

impl StreamingOperation for Messages {
    type StreamEvent = MessagesStreamEvent;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessagesRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [Message<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagesResponse {
    pub content: String,
    pub stop_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessagesStreamEvent {
    ContentDelta(String),
    Finished,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagesCall {
    pub model: String,
    pub messages: Vec<CallMessage>,
    pub system: Option<String>,
    pub parameters: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallMessage {
    pub role: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_exposes_the_canonical_contract() {
        fn request_model(request: <Messages as Operation>::Request<'_>) -> &str {
            request.model
        }

        assert_eq!(
            request_model(MessagesRequest {
                model: "model",
                messages: &[]
            }),
            "model"
        );
        assert_eq!(Messages::NAME, "messages");
    }
}
