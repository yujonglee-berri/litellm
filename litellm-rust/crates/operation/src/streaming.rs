use std::future::Future;

use futures_util::StreamExt;
use futures_util::stream::Stream;
use litellm_auth::{Auth, ResolvedAuth};
use litellm_transport::{FinalRequest, Transport};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Error;
use crate::codec::OperationCodec;
use crate::execution::{encode_request, ensure_success};
use crate::plan::Delivery;

const DONE_SENTINEL: &str = "[DONE]";

pub type OperationEventStream<E> = std::pin::Pin<Box<dyn Stream<Item = Result<E, Error>> + Send>>;

pub trait OperationStreamCodec: OperationCodec {
    type ServerEvent: DeserializeOwned;
    type StreamEvent;

    fn decode_event(
        &self,
        call: &Self::Call,
        event: Self::ServerEvent,
    ) -> Result<Option<Self::StreamEvent>, Error>;
}

pub trait ExecuteStreamOperation<C, A, AuthContext>: Send + Sync
where
    C: OperationStreamCodec + Clone + 'static,
    C::Call: 'static,
    A: Auth,
{
    fn execute_stream(
        &self,
        codec: C,
        call: C::Call,
        params: &C::Params,
        endpoint: &str,
        auth: &ResolvedAuth<A, AuthContext>,
    ) -> impl Future<Output = Result<OperationEventStream<C::StreamEvent>, Error>> + Send;
}

#[derive(Debug)]
pub struct SseExecution<T> {
    transport: T,
}

impl<T> SseExecution<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<C, A, AuthContext, T> ExecuteStreamOperation<C, A, AuthContext> for SseExecution<T>
where
    C: OperationStreamCodec + Clone + 'static,
    C::Call: 'static,
    C::WireRequest: Serialize,
    C::ServerEvent: DeserializeOwned,
    A: Auth,
    AuthContext: Send + Sync,
    T: Transport + Send + Sync,
{
    async fn execute_stream(
        &self,
        codec: C,
        call: C::Call,
        params: &C::Params,
        endpoint: &str,
        auth: &ResolvedAuth<A, AuthContext>,
    ) -> Result<OperationEventStream<C::StreamEvent>, Error> {
        let request = encode_request(&codec, &call, params, endpoint, auth, Delivery::Stream)?;
        let authenticated = FinalRequest::new(request)
            .authenticate(&auth.authenticator)
            .await?;
        let response = ensure_success(self.transport.send(authenticated).await?).await?;
        let events = futures_util::stream::try_unfold(
            (sse_data_frames(response), codec, call),
            |(mut frames, codec, call)| async move {
                loop {
                    let data = match frames.next().await {
                        Some(Ok(data)) => data,
                        Some(Err(error)) => return Err(error),
                        None => return Ok(None),
                    };
                    if data == DONE_SENTINEL {
                        continue;
                    }
                    let event = serde_json::from_str(&data).map_err(|error| {
                        Error::InvalidResponse(format!(
                            "stream event deserialization failed: {error}"
                        ))
                    })?;
                    match codec.decode_event(&call, event) {
                        Ok(Some(event)) => return Ok(Some((event, (frames, codec, call)))),
                        Ok(None) => continue,
                        Err(error) => return Err(error),
                    }
                }
            },
        );
        Ok(Box::pin(events))
    }
}

fn frame_end(buffer: &[u8]) -> Option<usize> {
    for index in 0..buffer.len() {
        if buffer[index..].starts_with(b"\r\n\r\n") {
            return Some(index + 4);
        }
        if buffer[index..].starts_with(b"\n\n") {
            return Some(index + 2);
        }
    }
    None
}

fn frame_data(frame: &[u8]) -> Option<String> {
    let mut payload: Vec<u8> = Vec::new();
    let mut saw_data = false;
    for line in frame.split(|&byte| byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.first() == Some(&b':') {
            continue;
        }
        let Some(value) = line.strip_prefix(b"data:") else {
            continue;
        };
        if saw_data {
            payload.push(b'\n');
        }
        payload.extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
        saw_data = true;
    }
    if saw_data {
        Some(String::from_utf8_lossy(&payload).into_owned())
    } else {
        None
    }
}

fn sse_data_frames(
    response: reqwest::Response,
) -> std::pin::Pin<Box<dyn Stream<Item = Result<String, Error>> + Send>> {
    Box::pin(futures_util::stream::try_unfold(
        (response.bytes_stream(), Vec::<u8>::new(), false),
        |(mut chunks, mut buffer, mut exhausted)| async move {
            loop {
                while let Some(end) = frame_end(&buffer) {
                    let frame: Vec<u8> = buffer.drain(..end).collect();
                    if let Some(data) = frame_data(&frame) {
                        return Ok(Some((data, (chunks, buffer, exhausted))));
                    }
                }
                if exhausted {
                    if buffer.is_empty() {
                        return Ok(None);
                    }
                    let remainder = std::mem::take(&mut buffer);
                    return match frame_data(&remainder) {
                        Some(data) => Ok(Some((data, (chunks, buffer, exhausted)))),
                        None => Ok(None),
                    };
                }
                match chunks.next().await {
                    Some(Ok(chunk)) => buffer.extend_from_slice(&chunk),
                    Some(Err(error)) => {
                        return Err(Error::Transport(litellm_transport::Error::from(error)));
                    }
                    None => exhausted = true,
                }
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use litellm_auth::{Auth, AuthFuture, AuthScheme};
    use litellm_transport::Authenticated;

    use super::*;

    type Log = Arc<Mutex<Vec<String>>>;

    #[derive(Clone)]
    struct SseTransport {
        log: Log,
    }

    impl Transport for SseTransport {
        async fn send(
            &self,
            _request: FinalRequest<Authenticated>,
        ) -> Result<reqwest::Response, litellm_transport::Error> {
            self.log.lock().unwrap().push("send".into());
            Ok(reqwest::Response::from(http::Response::new(
                concat!(
                    "event: message_start\n",
                    "data: {\"type\":\"message_start\"}\n\n",
                    "data: {\"type\":\"content_block_delta\",\"text\":\"Hello\"}\n\n",
                    ": keep-alive comment\r\n\r\n",
                    "data: {\"type\":\"content_block_delta\",\"text\":\" world\"}\n\n",
                    "data: [DONE]\n\n",
                    "data: {\"type\":\"message_stop\"}\n\n",
                )
                .as_bytes()
                .to_vec(),
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

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestServerEvent {
        #[serde(rename = "type")]
        kind: String,
        text: Option<String>,
    }

    #[derive(Debug, PartialEq)]
    enum TestStreamEvent {
        Delta(String),
        Finished,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TestWire;

    #[derive(Clone)]
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
    }

    impl OperationStreamCodec for TestCodec {
        type ServerEvent = TestServerEvent;
        type StreamEvent = TestStreamEvent;

        fn decode_event(
            &self,
            _call: &String,
            event: TestServerEvent,
        ) -> Result<Option<TestStreamEvent>, Error> {
            Ok(match event.kind.as_str() {
                "content_block_delta" => event.text.map(TestStreamEvent::Delta),
                "message_stop" => Some(TestStreamEvent::Finished),
                _ => None,
            })
        }
    }

    #[tokio::test]
    async fn sse_execution_streams_decoded_events_in_order() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let execution = SseExecution::new(SseTransport { log: log.clone() });

        let stream = execution
            .execute_stream(
                TestCodec,
                "call".to_string(),
                &(),
                "https://operation.test/stream",
                &ResolvedAuth {
                    authenticator: UnitAuth,
                    headers: Vec::new(),
                    context: (),
                },
            )
            .await
            .expect("stream starts");

        let events: Vec<_> = stream
            .map(|event| event.expect("event decodes"))
            .collect()
            .await;

        assert_eq!(
            events,
            vec![
                TestStreamEvent::Delta("Hello".into()),
                TestStreamEvent::Delta(" world".into()),
                TestStreamEvent::Finished,
            ]
        );
        assert_eq!(*log.lock().unwrap(), vec!["send"]);
    }

    #[test]
    fn frame_data_joins_multi_line_payloads_and_skips_comments() {
        let frame = b"event: delta\ndata: line one\ndata:line two\n: skip me\nignored: field";
        assert_eq!(frame_data(frame), Some("line one\nline two".to_string()));
        assert_eq!(frame_data(b"event: only"), None);
    }

    #[test]
    fn frame_end_detects_lf_and_crlf_boundaries() {
        assert_eq!(frame_end(b"data: x\n\ndata: y\n\n"), Some(9));
        assert_eq!(frame_end(b"data: x\r\n\r\ndata: y\r\n\r\n"), Some(11));
        assert_eq!(frame_end(b"data: x"), None);
    }
}
