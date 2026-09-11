use futures_util::StreamExt;
use litellm_auth::{Auth, ResolveAuth};

use crate::Error;
use crate::codec::OperationCodec;
use crate::endpoint::ResolveEndpoint;
use crate::execution::ExecuteOperation;
use crate::hooks::{
    CompleteHooks, FailureHook, NoHooks, StreamEventHook, StreamHooks, StreamIteratorHook,
    StreamLogHook,
};
use crate::streaming::{ExecuteStreamOperation, OperationEventStream, OperationStreamCodec};
use crate::target::{HttpTarget, TargetAuth, TargetEndpoint};

pub struct Pipeline<A, E, C, X, H = NoHooks> {
    auth: A,
    endpoint: E,
    codec: C,
    execution: X,
    hooks: H,
}

impl<A, E, C, X, H> Pipeline<A, E, C, X, H> {
    pub fn new(auth: A, endpoint: E, codec: C, execution: X, hooks: H) -> Self {
        Self {
            auth,
            endpoint,
            codec,
            execution,
            hooks,
        }
    }
}

impl<A: Auth + Clone, C, X, H> Pipeline<TargetAuth<A>, TargetEndpoint, C, X, H> {
    pub fn for_target(target: HttpTarget<A>, codec: C, execution: X, hooks: H) -> Self {
        Self::new(
            TargetAuth::new(target.authenticator),
            TargetEndpoint::new(target.endpoint),
            codec,
            execution,
            hooks,
        )
    }
}

impl<A, E, C, X, H> Pipeline<A, E, C, X, H>
where
    C: OperationCodec,
    A: ResolveAuth<C::Call, C::Context, Error = Error>,
    E: ResolveEndpoint<A::Context, C::Params, C::Call, C::Context>,
    X: ExecuteOperation<C, A::Authenticator, A::Context>,
    H: CompleteHooks<C::Call, C::Response>,
{
    pub async fn handle(&self, call: C::Call, context: &C::Context) -> Result<C::Response, Error> {
        let call = report_failure(&self.hooks, self.hooks.pre_call(call))?;
        let params = report_failure(&self.hooks, self.codec.params(&call))?;
        let authentication = report_failure(&self.hooks, self.auth.resolve(&call, context).await)?;
        let endpoint = report_failure(
            &self.hooks,
            self.endpoint
                .resolve(&call, context, &authentication.context, &params),
        )?;
        let call = report_failure(&self.hooks, self.hooks.during_call(call, &endpoint))?;
        let response = report_failure(
            &self.hooks,
            self.execution
                .execute(&self.codec, &call, &params, &endpoint, &authentication)
                .await,
        )?;
        let response = report_failure(&self.hooks, self.hooks.post_call(&call, response))?;
        self.hooks.on_success(&call, &response);
        Ok(response)
    }
}

impl<A, E, C, X, H> Pipeline<A, E, C, X, H>
where
    C: OperationStreamCodec + Clone + 'static,
    C::Call: 'static,
    C::StreamEvent: Send + 'static,
    A: ResolveAuth<C::Call, C::Context, Error = Error>,
    E: ResolveEndpoint<A::Context, C::Params, C::Call, C::Context>,
    X: ExecuteStreamOperation<C, A::Authenticator, A::Context>,
    H: StreamHooks<C::Call, C::StreamEvent> + Clone + 'static,
{
    pub async fn stream(
        &self,
        call: C::Call,
        context: &C::Context,
    ) -> Result<OperationEventStream<C::StreamEvent>, Error> {
        let call = report_failure(&self.hooks, self.hooks.pre_call(call))?;
        let params = report_failure(&self.hooks, self.codec.params(&call))?;
        let authentication = report_failure(&self.hooks, self.auth.resolve(&call, context).await)?;
        let endpoint = report_failure(
            &self.hooks,
            self.endpoint
                .resolve(&call, context, &authentication.context, &params),
        )?;
        let call = report_failure(&self.hooks, self.hooks.during_call(call, &endpoint))?;
        let stream = report_failure(
            &self.hooks,
            self.execution
                .execute_stream(
                    self.codec.clone(),
                    call,
                    &params,
                    &endpoint,
                    &authentication,
                )
                .await,
        )?;
        Ok(apply_stream_hooks(self.hooks.clone(), stream))
    }
}

fn report_failure<T, H: FailureHook>(hooks: &H, result: Result<T, Error>) -> Result<T, Error> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            hooks.on_failure(&error);
            Err(error)
        }
    }
}

fn apply_stream_hooks<H, Event>(
    hooks: H,
    stream: OperationEventStream<Event>,
) -> OperationEventStream<Event>
where
    H: StreamEventHook<Event>
        + StreamIteratorHook<Event>
        + StreamLogHook<Event>
        + FailureHook
        + 'static,
    Event: Send + 'static,
{
    let stream = hooks.wrap_stream(stream);
    Box::pin(stream.filter_map(move |item| {
        std::future::ready(match item {
            Err(error) => {
                hooks.on_failure(&error);
                Some(Err(error))
            }
            Ok(event) => match hooks.on_stream_event(event) {
                Ok(Some(event)) => {
                    hooks.log_stream_event(&event);
                    Some(Ok(event))
                }
                Ok(None) => None,
                Err(error) => {
                    hooks.on_failure(&error);
                    Some(Err(error))
                }
            },
        })
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use litellm_auth::{Auth, AuthFuture, AuthScheme};
    use litellm_transport::{Authenticated, FinalRequest, Transport};

    use super::*;
    use crate::{Delivery, Error, HttpTarget, JsonExecution, NoHooks, OperationCodec};
    use futures_util::StreamExt;

    type Log = Arc<Mutex<Vec<String>>>;

    #[derive(Clone)]
    struct RecordingAuth {
        log: Log,
    }

    impl Auth for RecordingAuth {
        fn scheme(&self) -> AuthScheme {
            AuthScheme::None
        }

        fn authenticate(&self, request: reqwest::Request) -> AuthFuture<'_> {
            let log = self.log.clone();
            Box::pin(async move {
                let protocol = request
                    .headers()
                    .get("x-protocol")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let content_type = request
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                log.lock()
                    .unwrap()
                    .push(format!("authenticate:{protocol}:{content_type}"));
                Ok(request)
            })
        }
    }

    #[derive(Clone)]
    struct RecordingTransport {
        log: Log,
    }

    impl Transport for RecordingTransport {
        async fn send(
            &self,
            request: FinalRequest<Authenticated>,
        ) -> Result<reqwest::Response, litellm_transport::Error> {
            self.log
                .lock()
                .unwrap()
                .push(format!("send:{}", request.request().url()));
            Ok(reqwest::Response::from(http::Response::new(
                b"null" as &'static [u8],
            )))
        }
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TestWire;

    struct TestCodec;

    impl OperationCodec for TestCodec {
        type Call = String;
        type Context = ();
        type Params = ();
        type WireRequest = TestWire;
        type WireResponse = TestWire;
        type Response = ();

        fn params(&self, _call: &String) -> Result<(), Error> {
            Ok(())
        }

        fn encode(
            &self,
            _call: &String,
            _params: &(),
            _delivery: Delivery,
        ) -> Result<TestWire, Error> {
            Ok(TestWire)
        }

        fn decode(&self, _call: &String, _response: TestWire) -> Result<(), Error> {
            Ok(())
        }

        fn protocol_headers(&self) -> Vec<(String, String)> {
            vec![("x-protocol".into(), "wire".into())]
        }
    }

    #[tokio::test]
    async fn for_target_sends_to_the_supplied_endpoint_after_protocol_headers() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::for_target(
            HttpTarget {
                endpoint: "https://openrouter.ai/api/v1/messages"
                    .parse()
                    .expect("url parses"),
                authenticator: RecordingAuth { log: log.clone() },
            },
            TestCodec,
            JsonExecution::new(RecordingTransport { log: log.clone() }),
            NoHooks,
        );

        pipeline
            .handle("call".into(), &())
            .await
            .expect("pipeline succeeds");

        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "authenticate:wire:application/json",
                "send:https://openrouter.ai/api/v1/messages",
            ]
        );
    }

    struct RecordingCompleteHooks {
        log: Log,
    }

    impl crate::PreCallHook<String> for RecordingCompleteHooks {
        fn pre_call(&self, call: String) -> Result<String, Error> {
            self.log.lock().unwrap().push("pre_call".into());
            Ok(format!("{call}:pre"))
        }
    }

    impl crate::DuringCallHook<String> for RecordingCompleteHooks {
        fn during_call(&self, call: String, endpoint: &str) -> Result<String, Error> {
            self.log
                .lock()
                .unwrap()
                .push(format!("during_call:{endpoint}"));
            Ok(call)
        }
    }

    impl crate::PostCallHook<String, ()> for RecordingCompleteHooks {
        fn post_call(&self, call: &String, response: ()) -> Result<(), Error> {
            self.log.lock().unwrap().push(format!("post_call:{call}"));
            Ok(response)
        }
    }

    impl crate::SuccessHook<String, ()> for RecordingCompleteHooks {
        fn on_success(&self, _call: &String, _response: &()) {
            self.log.lock().unwrap().push("on_success".into());
        }
    }

    impl crate::FailureHook for RecordingCompleteHooks {
        fn on_failure(&self, error: &Error) {
            self.log.lock().unwrap().push(format!("on_failure:{error}"));
        }
    }

    struct RejectingPreCall {
        log: Log,
    }

    impl crate::PreCallHook<String> for RejectingPreCall {
        fn pre_call(&self, _call: String) -> Result<String, Error> {
            Err(Error::InvalidRequest("blocked".into()))
        }
    }

    impl crate::DuringCallHook<String> for RejectingPreCall {}
    impl crate::PostCallHook<String, ()> for RejectingPreCall {}
    impl crate::SuccessHook<String, ()> for RejectingPreCall {}
    impl crate::FailureHook for RejectingPreCall {
        fn on_failure(&self, error: &Error) {
            self.log.lock().unwrap().push(format!("on_failure:{error}"));
        }
    }

    #[tokio::test]
    async fn handle_runs_complete_hooks_around_the_provider_call() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::for_target(
            HttpTarget {
                endpoint: "https://operation.test/complete"
                    .parse()
                    .expect("url parses"),
                authenticator: RecordingAuth { log: log.clone() },
            },
            TestCodec,
            JsonExecution::new(RecordingTransport { log: log.clone() }),
            RecordingCompleteHooks { log: log.clone() },
        );

        pipeline
            .handle("call".into(), &())
            .await
            .expect("pipeline succeeds");

        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "pre_call",
                "during_call:https://operation.test/complete",
                "authenticate:wire:application/json",
                "send:https://operation.test/complete",
                "post_call:call:pre",
                "on_success",
            ]
        );
    }

    #[tokio::test]
    async fn handle_reports_pre_call_failure_without_sending() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::for_target(
            HttpTarget {
                endpoint: "https://operation.test/complete"
                    .parse()
                    .expect("url parses"),
                authenticator: RecordingAuth { log: log.clone() },
            },
            TestCodec,
            JsonExecution::new(RecordingTransport { log: log.clone() }),
            RejectingPreCall { log: log.clone() },
        );

        let error = pipeline
            .handle("call".into(), &())
            .await
            .expect_err("pre_call blocks");

        assert_eq!(error.to_string(), "invalid request: blocked");
        assert_eq!(
            *log.lock().unwrap(),
            vec!["on_failure:invalid request: blocked"]
        );
    }

    #[derive(Clone)]
    struct RecordingStreamHooks {
        log: Log,
    }

    impl crate::PreCallHook<String> for RecordingStreamHooks {
        fn pre_call(&self, call: String) -> Result<String, Error> {
            self.log.lock().unwrap().push("pre_call".into());
            Ok(call)
        }
    }

    impl crate::DuringCallHook<String> for RecordingStreamHooks {
        fn during_call(&self, call: String, endpoint: &str) -> Result<String, Error> {
            self.log
                .lock()
                .unwrap()
                .push(format!("during_call:{endpoint}"));
            Ok(call)
        }
    }

    impl crate::StreamEventHook<String> for RecordingStreamHooks {
        fn on_stream_event(&self, event: String) -> Result<Option<String>, Error> {
            self.log
                .lock()
                .unwrap()
                .push(format!("on_stream_event:{event}"));
            Ok((event != "skip").then_some(event))
        }
    }

    impl crate::StreamIteratorHook<String> for RecordingStreamHooks {
        fn wrap_stream(
            &self,
            stream: crate::OperationEventStream<String>,
        ) -> crate::OperationEventStream<String> {
            self.log.lock().unwrap().push("wrap_stream".into());
            stream
        }
    }

    impl crate::StreamLogHook<String> for RecordingStreamHooks {
        fn log_stream_event(&self, event: &String) {
            self.log
                .lock()
                .unwrap()
                .push(format!("log_stream_event:{event}"));
        }
    }

    impl crate::FailureHook for RecordingStreamHooks {}

    #[derive(Clone)]
    struct StreamCodec;

    impl OperationCodec for StreamCodec {
        type Call = String;
        type Context = ();
        type Params = ();
        type WireRequest = TestWire;
        type WireResponse = TestWire;
        type Response = ();

        fn params(&self, _call: &String) -> Result<(), Error> {
            Ok(())
        }

        fn encode(
            &self,
            _call: &String,
            _params: &(),
            _delivery: Delivery,
        ) -> Result<TestWire, Error> {
            Ok(TestWire)
        }

        fn decode(&self, _call: &String, _response: TestWire) -> Result<(), Error> {
            Ok(())
        }

        fn protocol_headers(&self) -> Vec<(String, String)> {
            vec![("x-protocol".into(), "wire".into())]
        }
    }

    impl crate::OperationStreamCodec for StreamCodec {
        type ServerEvent = String;
        type StreamEvent = String;

        fn decode_event(&self, _call: &String, event: String) -> Result<Option<String>, Error> {
            Ok(Some(event))
        }
    }

    #[derive(Clone)]
    struct StreamTransport {
        log: Log,
    }

    impl Transport for StreamTransport {
        async fn send(
            &self,
            _request: FinalRequest<Authenticated>,
        ) -> Result<reqwest::Response, litellm_transport::Error> {
            self.log.lock().unwrap().push("send".into());
            Ok(reqwest::Response::from(http::Response::new(
                concat!(
                    "data: \"keep\"\n\n",
                    "data: \"skip\"\n\n",
                    "data: \"keep-too\"\n\n",
                )
                .as_bytes()
                .to_vec(),
            )))
        }
    }

    #[tokio::test]
    async fn stream_runs_stream_hooks_and_drops_filtered_events() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::for_target(
            HttpTarget {
                endpoint: "https://operation.test/stream".parse().expect("url parses"),
                authenticator: RecordingAuth { log: log.clone() },
            },
            StreamCodec,
            crate::SseExecution::new(StreamTransport { log: log.clone() }),
            RecordingStreamHooks { log: log.clone() },
        );

        let stream = pipeline
            .stream("call".into(), &())
            .await
            .expect("stream starts");
        let events: Vec<_> = stream
            .map(|event| event.expect("event decodes"))
            .collect()
            .await;

        assert_eq!(events, vec!["keep".to_string(), "keep-too".to_string()]);
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "pre_call",
                "during_call:https://operation.test/stream",
                "authenticate:wire:application/json",
                "send",
                "wrap_stream",
                "on_stream_event:keep",
                "log_stream_event:keep",
                "on_stream_event:skip",
                "on_stream_event:keep-too",
                "log_stream_event:keep-too",
            ]
        );
    }
}
