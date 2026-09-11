use std::future::Future;

use litellm_auth::{Auth, ResolvedAuth};
use litellm_transport::{Authenticated, FinalRequest, Transport};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Error;
use crate::codec::OperationCodec;
use crate::plan::Delivery;

pub trait ExecuteOperation<C, A, AuthContext>: Send + Sync
where
    C: OperationCodec,
    A: Auth,
{
    fn execute(
        &self,
        codec: &C,
        call: &C::Call,
        params: &C::Params,
        endpoint: &str,
        auth: &ResolvedAuth<A, AuthContext>,
    ) -> impl Future<Output = Result<C::Response, Error>> + Send;
}

#[derive(Debug)]
pub struct JsonExecution<T> {
    transport: T,
}

impl<T> JsonExecution<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

pub(crate) fn encode_request<C, A, AuthContext>(
    codec: &C,
    call: &C::Call,
    params: &C::Params,
    endpoint: &str,
    auth: &ResolvedAuth<A, AuthContext>,
    delivery: Delivery,
) -> Result<reqwest::Request, Error>
where
    C: OperationCodec,
    C::WireRequest: Serialize,
    A: Auth,
{
    let wire_request = codec.encode(call, params, delivery)?;
    let body = serde_json::to_vec(&wire_request)
        .map_err(|error| Error::InvalidRequest(format!("request serialization failed: {error}")))?;
    let url = endpoint
        .parse()
        .map_err(|error| Error::InvalidRequest(format!("endpoint url is invalid: {error}")))?;
    let mut request = reqwest::Request::new(reqwest::Method::POST, url);
    *request.body_mut() = Some(body.into());
    request.headers_mut().insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    apply_headers(request.headers_mut(), &codec.protocol_headers())?;
    apply_headers(request.headers_mut(), &auth.headers)?;
    Ok(request)
}

fn apply_headers(
    headers: &mut reqwest::header::HeaderMap,
    values: &[(String, String)],
) -> Result<(), Error> {
    for (name, value) in values {
        let name: reqwest::header::HeaderName = name
            .parse()
            .map_err(|error| Error::InvalidRequest(format!("invalid header name: {error}")))?;
        let header_value: reqwest::header::HeaderValue = value
            .parse()
            .map_err(|error| Error::InvalidRequest(format!("invalid header value: {error}")))?;
        headers.insert(name, header_value);
    }
    Ok(())
}

pub(crate) async fn ensure_success(
    response: reqwest::Response,
) -> Result<reqwest::Response, Error> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message = response.text().await.unwrap_or_default();
    Err(Error::Provider {
        status: status.as_u16(),
        message,
    })
}

impl<C, A, AuthContext, T> ExecuteOperation<C, A, AuthContext> for JsonExecution<T>
where
    C: OperationCodec,
    C::WireRequest: Serialize,
    C::WireResponse: DeserializeOwned,
    A: Auth,
    AuthContext: Send + Sync,
    T: Transport + Send + Sync,
{
    async fn execute(
        &self,
        codec: &C,
        call: &C::Call,
        params: &C::Params,
        endpoint: &str,
        auth: &ResolvedAuth<A, AuthContext>,
    ) -> Result<C::Response, Error> {
        let request = encode_request(codec, call, params, endpoint, auth, Delivery::Complete)?;
        let authenticated: FinalRequest<Authenticated> = FinalRequest::new(request)
            .authenticate(&auth.authenticator)
            .await?;
        let response = ensure_success(self.transport.send(authenticated).await?).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(litellm_transport::Error::from)?;
        let wire_response: C::WireResponse = serde_json::from_slice(&bytes).map_err(|error| {
            Error::InvalidResponse(format!("response deserialization failed: {error}"))
        })?;
        codec.decode(call, wire_response)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use litellm_auth::{Auth, AuthFuture, AuthScheme};
    use litellm_transport::Transport;

    use super::*;

    type Log = Arc<Mutex<Vec<String>>>;

    #[derive(Clone)]
    struct LoggingTransport {
        log: Log,
    }

    impl Transport for LoggingTransport {
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

    struct UnitAuth;

    impl Auth for UnitAuth {
        fn scheme(&self) -> AuthScheme {
            AuthScheme::None
        }

        fn authenticate(&self, request: reqwest::Request) -> AuthFuture<'_> {
            Box::pin(async move { Ok(request) })
        }
    }

    #[derive(Debug, PartialEq)]
    struct TestResponse {
        seen: bool,
    }

    struct TestCodec;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TestWire;

    impl OperationCodec for TestCodec {
        type Call = String;
        type Context = ();
        type Params = ();
        type WireRequest = TestWire;
        type WireResponse = TestWire;
        type Response = TestResponse;

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

        fn decode(&self, _call: &String, _response: TestWire) -> Result<TestResponse, Error> {
            Ok(TestResponse { seen: true })
        }
    }

    #[tokio::test]
    async fn json_execution_authenticates_then_sends_then_decodes() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let execution = JsonExecution::new(LoggingTransport { log: log.clone() });

        let response = execution
            .execute(
                &TestCodec,
                &"call".to_string(),
                &(),
                "https://operation.test/complete",
                &ResolvedAuth {
                    authenticator: UnitAuth,
                    headers: Vec::new(),
                    context: (),
                },
            )
            .await
            .expect("execution succeeds");

        assert_eq!(response, TestResponse { seen: true });
        assert_eq!(*log.lock().unwrap(), vec!["send"]);
    }
}
