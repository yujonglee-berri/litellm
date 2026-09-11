use std::sync::{Arc, Mutex};

use litellm_auth::{Auth, AuthFuture, AuthScheme, ResolveAuth, ResolvedAuth};
use litellm_operation::{
    Delivery, DuringCallHook, Error, FailureHook, JsonExecution, NoHooks, OperationCodec, Pipeline,
    PostCallHook, PreCallHook, ResolveEndpoint, SuccessHook,
};
use litellm_operation_ocr::{OcrCall, OcrCallContext, OcrDocument, OcrResponse};
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

impl PreCallHook<OcrCall> for RecordingHook {
    fn pre_call(&self, mut call: OcrCall) -> Result<OcrCall, Error> {
        self.log.lock().unwrap().push("pre_call".into());
        call.document = OcrDocument::Uri("https://rewritten.test/doc".into());
        Ok(call)
    }
}

impl DuringCallHook<OcrCall> for RecordingHook {}
impl PostCallHook<OcrCall, OcrResponse> for RecordingHook {}
impl SuccessHook<OcrCall, OcrResponse> for RecordingHook {}
impl FailureHook for RecordingHook {}

struct FakeResolver {
    log: Log,
}

impl ResolveAuth<OcrCall, OcrCallContext> for FakeResolver {
    type Authenticator = LoggingAuth;
    type Context = ();
    type Error = Error;

    async fn resolve(
        &self,
        _call: &OcrCall,
        _context: &OcrCallContext,
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

impl ResolveEndpoint<(), String, OcrCall, OcrCallContext> for FakeEndpoint {
    fn resolve(
        &self,
        _call: &OcrCall,
        _context: &OcrCallContext,
        _auth: &(),
        _params: &String,
    ) -> Result<String, Error> {
        self.log.lock().unwrap().push("endpoint".into());
        Ok("https://ocr.test/analyze".into())
    }
}

struct FakeCodec {
    log: Log,
}

impl OperationCodec for FakeCodec {
    type Call = OcrCall;
    type Context = OcrCallContext;
    type Params = String;
    type WireRequest = Value;
    type WireResponse = Value;
    type Response = OcrResponse;

    fn params(&self, call: &OcrCall) -> Result<String, Error> {
        let level = call
            .parameters
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or("standard")
            .to_string();
        self.log.lock().unwrap().push(format!("params:{level}"));
        Ok(level)
    }

    fn encode(&self, call: &OcrCall, params: &String, _delivery: Delivery) -> Result<Value, Error> {
        let uri = call.document.uri().unwrap_or_default().to_string();
        self.log.lock().unwrap().push(format!("encode:{uri}"));
        Ok(json!({ "url": uri, "level": params }))
    }

    fn decode(&self, _call: &OcrCall, _response: Value) -> Result<OcrResponse, Error> {
        self.log.lock().unwrap().push("decode".into());
        Ok(OcrResponse { pages: Vec::new() })
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
            b"{\"ok\":true}" as &'static [u8],
        )))
    }
}

#[tokio::test]
async fn pipeline_orders_hooks_params_auth_endpoint_encode_authenticate_send_decode() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Pipeline::new(
        FakeResolver { log: log.clone() },
        FakeEndpoint { log: log.clone() },
        FakeCodec { log: log.clone() },
        JsonExecution::new(FakeTransport { log: log.clone() }),
        RecordingHook { log: log.clone() },
    );

    let mut parameters = Map::new();
    parameters.insert("level".into(), json!("detailed"));
    let call = OcrCall {
        model: "ocr-test".into(),
        document: OcrDocument::Uri("https://original.test/doc".into()),
        parameters,
    };

    pipeline
        .handle(call, &OcrCallContext::default())
        .await
        .expect("pipeline succeeds");

    let recorded = log.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            "pre_call",
            "params:detailed",
            "auth",
            "endpoint",
            "encode:https://rewritten.test/doc",
            "authenticate",
            "send",
            "decode",
        ]
    );
}

#[tokio::test]
async fn no_hooks_pass_the_call_through() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Pipeline::new(
        FakeResolver { log: log.clone() },
        FakeEndpoint { log: log.clone() },
        FakeCodec { log: log.clone() },
        JsonExecution::new(FakeTransport { log: log.clone() }),
        NoHooks,
    );

    let call = OcrCall {
        model: "ocr-test".into(),
        document: OcrDocument::Uri("https://original.test/doc".into()),
        parameters: Map::new(),
    };

    pipeline
        .handle(call, &OcrCallContext::default())
        .await
        .expect("pipeline succeeds");

    let recorded = log.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            "params:standard",
            "auth",
            "endpoint",
            "encode:https://original.test/doc",
            "authenticate",
            "send",
            "decode"
        ]
    );
}
