use crate::Error;
use crate::streaming::OperationEventStream;

pub trait PreCallHook<Call>: Send + Sync {
    fn pre_call(&self, call: Call) -> Result<Call, Error> {
        Ok(call)
    }
}

pub trait DuringCallHook<Call>: Send + Sync {
    fn during_call(&self, call: Call, _endpoint: &str) -> Result<Call, Error> {
        Ok(call)
    }
}

pub trait PostCallHook<Call, Response>: Send + Sync {
    fn post_call(&self, _call: &Call, response: Response) -> Result<Response, Error> {
        Ok(response)
    }
}

pub trait SuccessHook<Call, Response>: Send + Sync {
    fn on_success(&self, _call: &Call, _response: &Response) {}
}

pub trait FailureHook: Send + Sync {
    fn on_failure(&self, _error: &Error) {}
}

pub trait StreamEventHook<Event>: Send + Sync {
    fn on_stream_event(&self, event: Event) -> Result<Option<Event>, Error> {
        Ok(Some(event))
    }
}

pub trait StreamIteratorHook<Event>: Send + Sync {
    fn wrap_stream(&self, stream: OperationEventStream<Event>) -> OperationEventStream<Event>
    where
        Event: Send + 'static,
    {
        stream
    }
}

pub trait StreamLogHook<Event>: Send + Sync {
    fn log_stream_event(&self, _event: &Event) {}
}

pub trait CompleteHooks<Call, Response>:
    PreCallHook<Call>
    + DuringCallHook<Call>
    + PostCallHook<Call, Response>
    + SuccessHook<Call, Response>
    + FailureHook
{
}

impl<T, Call, Response> CompleteHooks<Call, Response> for T where
    T: PreCallHook<Call>
        + DuringCallHook<Call>
        + PostCallHook<Call, Response>
        + SuccessHook<Call, Response>
        + FailureHook
{
}

pub trait StreamHooks<Call, Event>:
    PreCallHook<Call>
    + DuringCallHook<Call>
    + StreamEventHook<Event>
    + StreamIteratorHook<Event>
    + StreamLogHook<Event>
    + FailureHook
{
}

impl<T, Call, Event> StreamHooks<Call, Event> for T where
    T: PreCallHook<Call>
        + DuringCallHook<Call>
        + StreamEventHook<Event>
        + StreamIteratorHook<Event>
        + StreamLogHook<Event>
        + FailureHook
{
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoHooks;

impl<Call> PreCallHook<Call> for NoHooks {}
impl<Call> DuringCallHook<Call> for NoHooks {}
impl<Call, Response> PostCallHook<Call, Response> for NoHooks {}
impl<Call, Response> SuccessHook<Call, Response> for NoHooks {}
impl FailureHook for NoHooks {}
impl<Event> StreamEventHook<Event> for NoHooks {}
impl<Event> StreamIteratorHook<Event> for NoHooks {}
impl<Event> StreamLogHook<Event> for NoHooks {}

#[cfg(test)]
mod tests {
    use super::*;

    struct CompleteOnly;

    impl PreCallHook<String> for CompleteOnly {
        fn pre_call(&self, call: String) -> Result<String, Error> {
            Ok(format!("{call}:pre"))
        }
    }
    impl DuringCallHook<String> for CompleteOnly {}
    impl PostCallHook<String, String> for CompleteOnly {
        fn post_call(&self, _call: &String, response: String) -> Result<String, Error> {
            Ok(format!("{response}:post"))
        }
    }
    impl SuccessHook<String, String> for CompleteOnly {}
    impl FailureHook for CompleteOnly {}

    struct StreamOnly;

    impl PreCallHook<String> for StreamOnly {}
    impl DuringCallHook<String> for StreamOnly {}
    impl StreamEventHook<u8> for StreamOnly {
        fn on_stream_event(&self, event: u8) -> Result<Option<u8>, Error> {
            Ok(event.is_multiple_of(2).then_some(event))
        }
    }
    impl StreamIteratorHook<u8> for StreamOnly {}
    impl StreamLogHook<u8> for StreamOnly {}
    impl FailureHook for StreamOnly {}

    fn assert_complete<H: CompleteHooks<String, String>>(_: &H) {}
    fn assert_stream<H: StreamHooks<String, u8>>(_: &H) {}

    #[test]
    fn no_hooks_satisfy_both_deliveries() {
        assert_complete(&NoHooks);
        assert_stream(&NoHooks);
    }

    #[test]
    fn complete_hooks_rewrite_the_call_and_response() {
        let hooks = CompleteOnly;
        assert_complete(&hooks);
        let call = hooks.pre_call("call".into()).expect("pre_call");
        let call = hooks.during_call(call, "https://example").expect("during");
        let response = hooks.post_call(&call, "ok".into()).expect("post_call");
        hooks.on_success(&call, &response);
        assert_eq!(call, "call:pre");
        assert_eq!(response, "ok:post");
    }

    #[test]
    fn stream_hooks_can_drop_events() {
        let hooks = StreamOnly;
        assert_stream(&hooks);
        assert_eq!(hooks.on_stream_event(1).expect("odd"), None);
        assert_eq!(hooks.on_stream_event(2).expect("even"), Some(2));
    }
}
