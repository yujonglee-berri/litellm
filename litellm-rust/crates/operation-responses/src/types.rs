use litellm_operation::{Operation, SessionOperation, StreamingOperation};

use crate::wire::ResponsesSessionEvent;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Responses;

impl Operation for Responses {
    type Request<'a> = ResponsesRequest<'a>;
    type Response = ResponsesResponse;

    const NAME: &'static str = "responses";
}

impl StreamingOperation for Responses {
    type StreamEvent = ResponsesStreamEvent;
}

impl SessionOperation for Responses {
    type ClientEvent = ResponsesSessionEvent;
    type ServerEvent = ResponsesSessionEvent;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponsesRequest<'a> {
    pub model: &'a str,
    pub input: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponsesResponse {
    pub id: String,
    pub output_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResponsesStreamEvent {
    OutputTextDelta(String),
    Completed(ResponsesResponse),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::ResponsesSessionEventType;
    use litellm_operation::SessionOperation;

    #[test]
    fn responses_exposes_complete_stream_and_session_contracts() {
        fn complete(response: <Responses as Operation>::Response) -> String {
            response.output_text
        }
        fn stream(response: ResponsesResponse) -> <Responses as StreamingOperation>::StreamEvent {
            ResponsesStreamEvent::Completed(response)
        }
        fn session_create() -> <Responses as SessionOperation>::ClientEvent {
            ResponsesSessionEvent {
                event_type: ResponsesSessionEventType::ResponseCreate,
                data: serde_json::Map::new(),
            }
        }

        let response = ResponsesResponse {
            id: "response-1".into(),
            output_text: "hello".into(),
        };
        assert_eq!(complete(response.clone()), "hello");
        assert_eq!(
            stream(response.clone()),
            ResponsesStreamEvent::Completed(response)
        );
        assert!(session_create().is_response_create());
    }
}
