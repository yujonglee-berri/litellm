use litellm_operation::{Operation, StreamingOperation};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Realtime;

impl Operation for Realtime {
    type Request<'a> = RealtimeRequest<'a>;
    type Response = RealtimeSession;

    const NAME: &'static str = "realtime";
}

impl StreamingOperation for Realtime {
    type StreamEvent = RealtimeEvent;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeRequest<'a> {
    pub model: &'a str,
    pub modalities: &'a [Modality],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Modality {
    Text,
    Audio,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealtimeSession {
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RealtimeEvent {
    TextDelta(String),
    AudioDelta(Vec<u8>),
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_has_a_typed_stream_event() {
        fn event(value: &str) -> <Realtime as StreamingOperation>::StreamEvent {
            RealtimeEvent::TextDelta(value.into())
        }

        assert_eq!(event("delta"), RealtimeEvent::TextDelta("delta".into()));
    }
}
