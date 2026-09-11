use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use litellm_auth::{
    Auth, AuthFuture, AuthResolver, AuthScheme, ExistingHeaderBehavior, HeaderAuth, ResolvedAuth,
    SecretValue,
};
use litellm_operation::{
    Delivery, DuringCallHook, Error, FailureHook, HttpTarget, JsonExecution, NoHooks,
    OperationCodec, Pipeline, PostCallHook, PreCallHook, ResolveEndpoint, SseExecution,
    SuccessHook,
};
use litellm_operation_messages::{
    AnthropicMessagesWire, CallMessage, MessagesCall, MessagesResponse, MessagesStreamEvent,
};
use litellm_transport::{Authenticated, FinalRequest, Transport};
use serde_json::{Map, Value, json};

type Log = Arc<Mutex<Vec<String>>>;

#[derive(Clone)]
struct LoggingAuth {
    log: Log,
}

impl Auth for LoggingAuth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::None
    }

    fn authenticate(&self, request: reqwest::Request) -> AuthFuture<'_> {
        let log = self.log.clone();
        Box::pin(async move {
            log.lock().unwrap().push("authenticate".into());
            Ok(request)
        })
    }
}

struct RecordingHook {
    log: Log,
}

impl PreCallHook<MessagesCall> for RecordingHook {
    fn pre_call(&self, mut call: MessagesCall) -> Result<MessagesCall, Error> {
        self.log.lock().unwrap().push("pre_call".into());
        call.system = Some("guarded".into());
        Ok(call)
    }
}

impl DuringCallHook<MessagesCall> for RecordingHook {}
impl PostCallHook<MessagesCall, MessagesResponse> for RecordingHook {}
impl SuccessHook<MessagesCall, MessagesResponse> for RecordingHook {}
impl FailureHook for RecordingHook {}

struct FakeResolver {
    log: Log,
}

impl AuthResolver<MessagesCall, ()> for FakeResolver {
    type Authenticator = LoggingAuth;
    type AuthContext = ();
    type Error = Error;

    async fn resolve(
        &self,
        _call: &MessagesCall,
        _context: &(),
    ) -> Result<ResolvedAuth<LoggingAuth, ()>, Error> {
        self.log.lock().unwrap().push("auth".into());
        Ok(ResolvedAuth {
            authenticator: LoggingAuth {
                log: self.log.clone(),
            },
            headers: Vec::new(),
            context: (),
        })
    }
}

struct FakeEndpoint {
    log: Log,
}

impl ResolveEndpoint<(), Value, MessagesCall, ()> for FakeEndpoint {
    fn resolve(
        &self,
        _call: &MessagesCall,
        _context: &(),
        _auth: &(),
        _params: &Value,
    ) -> Result<String, Error> {
        self.log.lock().unwrap().push("endpoint".into());
        Ok("https://messages.test/v1/messages".into())
    }
}

struct FakeCodec {
    log: Log,
}

impl OperationCodec for FakeCodec {
    type Call = MessagesCall;
    type Context = ();
    type Params = Value;
    type WireRequest = Value;
    type WireResponse = Value;
    type Response = MessagesResponse;

    fn params(&self, call: &MessagesCall) -> Result<Value, Error> {
        let max_tokens = call
            .parameters
            .get("max_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(64);
        self.log
            .lock()
            .unwrap()
            .push(format!("params:{max_tokens}"));
        Ok(json!({ "max_tokens": max_tokens }))
    }

    fn encode(
        &self,
        call: &MessagesCall,
        params: &Value,
        _delivery: Delivery,
    ) -> Result<Value, Error> {
        let system = call.system.as_deref().unwrap_or_default();
        self.log
            .lock()
            .unwrap()
            .push(format!("encode:{system}:{}", call.messages.len()));
        Ok(json!({ "system": system, "params": params }))
    }

    fn decode(&self, _call: &MessagesCall, _response: Value) -> Result<MessagesResponse, Error> {
        self.log.lock().unwrap().push("decode".into());
        Ok(MessagesResponse {
            content: "done".into(),
            stop_reason: None,
        })
    }
}

#[derive(Clone)]
struct FakeTransport {
    log: Log,
}

impl Transport for FakeTransport {
    async fn send(
        &self,
        _request: FinalRequest<Authenticated>,
    ) -> Result<reqwest::Response, litellm_transport::Error> {
        self.log.lock().unwrap().push("send".into());
        Ok(reqwest::Response::from(http::Response::new(
            b"null" as &'static [u8],
        )))
    }
}

fn call(parameters: Map<String, Value>) -> MessagesCall {
    MessagesCall {
        model: "anthropic/claude-haiku-4.5".into(),
        messages: vec![CallMessage {
            role: "user".into(),
            content: "hello".into(),
        }],
        system: None,
        parameters,
    }
}

#[tokio::test]
async fn shared_pipeline_orders_hooks_params_auth_endpoint_encode_authenticate_send_decode() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Pipeline::new(
        FakeResolver { log: log.clone() },
        FakeEndpoint { log: log.clone() },
        FakeCodec { log: log.clone() },
        JsonExecution::new(FakeTransport { log: log.clone() }),
        RecordingHook { log: log.clone() },
    );

    let mut parameters = Map::new();
    parameters.insert("max_tokens".into(), json!(256));
    let response = pipeline
        .handle(call(parameters), &())
        .await
        .expect("pipeline succeeds");

    assert_eq!(response.content, "done");
    let recorded = log.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            "pre_call",
            "params:256",
            "auth",
            "endpoint",
            "encode:guarded:1",
            "authenticate",
            "send",
            "decode",
        ]
    );
}

#[derive(Clone)]
struct InspectingAuth {
    log: Log,
}

impl Auth for InspectingAuth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::None
    }

    fn authenticate(&self, request: reqwest::Request) -> AuthFuture<'_> {
        let log = self.log.clone();
        Box::pin(async move {
            let version = request
                .headers()
                .get("anthropic-version")
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
                .push(format!("authenticate:{version}:{content_type}"));
            Ok(request)
        })
    }
}

#[derive(Clone)]
struct InspectingTransport {
    log: Log,
    body: &'static [u8],
}

impl Transport for InspectingTransport {
    async fn send(
        &self,
        request: FinalRequest<Authenticated>,
    ) -> Result<reqwest::Response, litellm_transport::Error> {
        let payload = request
            .request()
            .body()
            .and_then(reqwest::Body::as_bytes)
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        let model = payload
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let stream = payload
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.log.lock().unwrap().push(format!(
            "send:{}:model={model}:stream={stream}",
            request.request().url()
        ));
        Ok(reqwest::Response::from(http::Response::new(
            self.body.to_vec(),
        )))
    }
}

const COMPLETE_BODY: &[u8] =
    br#"{"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn"}"#;

#[tokio::test]
async fn for_target_keeps_model_and_endpoint_independent_of_the_wire_adapter() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let openrouter = Pipeline::for_target(
        HttpTarget {
            endpoint: "https://openrouter.ai/api/v1/messages"
                .parse()
                .expect("url parses"),
            authenticator: InspectingAuth { log: log.clone() },
        },
        AnthropicMessagesWire::new("2023-06-01"),
        JsonExecution::new(InspectingTransport {
            log: log.clone(),
            body: COMPLETE_BODY,
        }),
        NoHooks,
    );
    let anthropic = Pipeline::for_target(
        HttpTarget {
            endpoint: "https://api.anthropic.com/v1/messages"
                .parse()
                .expect("url parses"),
            authenticator: InspectingAuth { log: log.clone() },
        },
        AnthropicMessagesWire::new("2023-06-01"),
        JsonExecution::new(InspectingTransport {
            log: log.clone(),
            body: COMPLETE_BODY,
        }),
        NoHooks,
    );

    let mut parameters = Map::new();
    parameters.insert("stream".into(), json!(true));
    openrouter
        .handle(call(parameters.clone()), &())
        .await
        .expect("openrouter completes");
    anthropic
        .handle(call(parameters), &())
        .await
        .expect("anthropic completes");

    assert_eq!(
        *log.lock().unwrap(),
        vec![
            "authenticate:2023-06-01:application/json",
            "send:https://openrouter.ai/api/v1/messages:model=anthropic/claude-haiku-4.5:stream=false",
            "authenticate:2023-06-01:application/json",
            "send:https://api.anthropic.com/v1/messages:model=anthropic/claude-haiku-4.5:stream=false",
        ]
    );
}

#[derive(Clone)]
struct SseTransport {
    log: Log,
}

impl Transport for SseTransport {
    async fn send(
        &self,
        request: FinalRequest<Authenticated>,
    ) -> Result<reqwest::Response, litellm_transport::Error> {
        let stream_flag = request
            .request()
            .body()
            .and_then(reqwest::Body::as_bytes)
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .and_then(|value| value.get("stream").and_then(Value::as_bool))
            .unwrap_or(false);
        self.log
            .lock()
            .unwrap()
            .push(format!("send:stream={stream_flag}"));
        Ok(reqwest::Response::from(http::Response::new(
            concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{}}\n\n",
                "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n",
                "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
                "data: {\"type\":\"message_stop\"}\n\n",
            )
            .as_bytes()
            .to_vec(),
        )))
    }
}

#[tokio::test]
async fn stream_execution_sets_the_wire_stream_flag_without_patching_the_call() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let authenticator = HeaderAuth::bearer(
        &SecretValue::new("openrouter-key"),
        ExistingHeaderBehavior::Preserve,
    )
    .expect("bearer auth");
    let pipeline = Pipeline::for_target(
        HttpTarget {
            endpoint: "https://openrouter.ai/api/v1/messages"
                .parse()
                .expect("url parses"),
            authenticator,
        },
        AnthropicMessagesWire::new("2023-06-01"),
        SseExecution::new(SseTransport { log: log.clone() }),
        NoHooks,
    );

    let stream = pipeline
        .stream(call(Map::new()), &())
        .await
        .unwrap_or_else(|error| panic!("stream starts: {error:?}"));

    let events: Vec<_> = stream
        .map(|event| event.expect("event decodes"))
        .collect()
        .await;

    assert_eq!(*log.lock().unwrap(), vec!["send:stream=true"]);
    assert_eq!(
        events,
        vec![
            MessagesStreamEvent::ContentDelta("Hello".into()),
            MessagesStreamEvent::ContentDelta(" world".into()),
            MessagesStreamEvent::Finished,
        ]
    );
}
