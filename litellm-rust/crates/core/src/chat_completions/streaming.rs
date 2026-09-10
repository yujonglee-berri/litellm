use std::collections::{BTreeMap, VecDeque};

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::client::http_client;
use super::execution::auth_error;
use super::registry::{ChatCompletionsTransformationKind, ChatCompletionsWireOperation};
use super::transformations::{ChatCompletionsStreamState, transform_stream_event};
use super::types::{
    ChatCompletionsStream, ChatCompletionsStreamEvent, ProviderChatCompletionsRequest,
};
use crate::auth::Auth;
use crate::error::Error;
use crate::http_utils::{http_request, truncate_error_body};
use crate::operation::OperationPlan;

const MAX_STREAM_FRAME_BYTES: usize = 16 * 1024 * 1024;

struct WireEvent {
    name: String,
    data: Value,
}

enum WireDecoder {
    Anthropic(SseDecoder),
    #[cfg(feature = "bedrock-auth")]
    Bedrock(AwsEventStreamDecoder),
}

impl WireDecoder {
    fn new(operation: ChatCompletionsWireOperation) -> Self {
        match operation {
            ChatCompletionsWireOperation::AnthropicMessages => {
                Self::Anthropic(SseDecoder::default())
            }
            #[cfg(feature = "bedrock-auth")]
            ChatCompletionsWireOperation::BedrockConverse => {
                Self::Bedrock(AwsEventStreamDecoder::default())
            }
        }
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<WireEvent>, Error> {
        match self {
            Self::Anthropic(decoder) => decoder.push(chunk),
            #[cfg(feature = "bedrock-auth")]
            Self::Bedrock(decoder) => decoder.push(chunk),
        }
    }

    fn finish(&mut self) -> Result<Vec<WireEvent>, Error> {
        match self {
            Self::Anthropic(decoder) => decoder.finish(),
            #[cfg(feature = "bedrock-auth")]
            Self::Bedrock(decoder) => decoder.finish(),
        }
    }
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<WireEvent>, Error> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > MAX_STREAM_FRAME_BYTES {
            return Err(stream_error("SSE frame exceeds the size limit"));
        }
        let mut events = Vec::new();
        while let Some((end, delimiter)) = sse_boundary(&self.buffer) {
            let frame = self.buffer.drain(..end).collect::<Vec<_>>();
            self.buffer.drain(..delimiter);
            if let Some(event) = decode_sse_frame(&frame)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<WireEvent>, Error> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let frame = std::mem::take(&mut self.buffer);
        Ok(decode_sse_frame(&frame)?.into_iter().collect())
    }
}

fn sse_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|end| (end, 2))
        .or_else(|| {
            buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|end| (end, 4))
        })
}

fn decode_sse_frame(frame: &[u8]) -> Result<Option<WireEvent>, Error> {
    let frame = std::str::from_utf8(frame).map_err(|_| stream_error("SSE frame is not UTF-8"))?;
    let mut name = None;
    let mut data = Vec::new();
    for line in frame.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            name = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start());
        }
    }
    if data.is_empty() || data == ["[DONE]"] {
        return Ok(None);
    }
    let data = serde_json::from_str(&data.join("\n"))
        .map_err(|_| stream_error("SSE data is not valid JSON"))?;
    Ok(Some(WireEvent {
        name: name.unwrap_or_else(|| "message".into()),
        data,
    }))
}

#[derive(Default)]
struct AwsEventStreamDecoder {
    buffer: Vec<u8>,
}

impl AwsEventStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<WireEvent>, Error> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        loop {
            if self.buffer.len() < 12 {
                break;
            }
            let total = read_u32(&self.buffer[..4]) as usize;
            let headers_len = read_u32(&self.buffer[4..8]) as usize;
            if !(16..=MAX_STREAM_FRAME_BYTES).contains(&total) || headers_len > total - 16 {
                return Err(stream_error("invalid AWS event-stream frame length"));
            }
            if self.buffer.len() < total {
                break;
            }
            let frame = self.buffer.drain(..total).collect::<Vec<_>>();
            validate_crc(&frame)?;
            let headers = decode_aws_headers(&frame[12..12 + headers_len])?;
            let payload = &frame[12 + headers_len..total - 4];
            let data = if payload.is_empty() {
                json!({})
            } else {
                serde_json::from_slice(payload)
                    .map_err(|_| stream_error("AWS event-stream payload is not valid JSON"))?
            };
            let name = headers
                .get(":event-type")
                .or_else(|| headers.get(":exception-type"))
                .or_else(|| headers.get(":message-type"))
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            events.push(WireEvent { name, data });
        }
        if self.buffer.len() > MAX_STREAM_FRAME_BYTES {
            return Err(stream_error(
                "AWS event-stream frame exceeds the size limit",
            ));
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<WireEvent>, Error> {
        if self.buffer.is_empty() {
            Ok(Vec::new())
        } else {
            Err(stream_error("truncated AWS event-stream frame"))
        }
    }
}

fn validate_crc(frame: &[u8]) -> Result<(), Error> {
    let prelude_crc = read_u32(&frame[8..12]);
    let message_crc = read_u32(&frame[frame.len() - 4..]);
    if crc32fast::hash(&frame[..8]) != prelude_crc
        || crc32fast::hash(&frame[..frame.len() - 4]) != message_crc
    {
        return Err(stream_error("AWS event-stream CRC mismatch"));
    }
    Ok(())
}

fn decode_aws_headers(mut input: &[u8]) -> Result<BTreeMap<String, String>, Error> {
    let mut headers = BTreeMap::new();
    while !input.is_empty() {
        let name_len = take(&mut input, 1)?[0] as usize;
        let name = std::str::from_utf8(take(&mut input, name_len)?)
            .map_err(|_| stream_error("AWS event-stream header name is not UTF-8"))?;
        let value_type = take(&mut input, 1)?[0];
        let value = match value_type {
            0 | 1 => None,
            2 => skip(&mut input, 1)?,
            3 => skip(&mut input, 2)?,
            4 => skip(&mut input, 4)?,
            5 | 8 => skip(&mut input, 8)?,
            6 | 7 => {
                let length = read_u16(take(&mut input, 2)?) as usize;
                let bytes = take(&mut input, length)?;
                (value_type == 7)
                    .then(|| std::str::from_utf8(bytes))
                    .transpose()
                    .map_err(|_| stream_error("AWS event-stream header value is not UTF-8"))?
                    .map(str::to_string)
            }
            9 => skip(&mut input, 16)?,
            _ => return Err(stream_error("unknown AWS event-stream header type")),
        };
        if let Some(value) = value {
            headers.insert(name.to_string(), value);
        }
    }
    Ok(headers)
}

fn skip(input: &mut &[u8], length: usize) -> Result<Option<String>, Error> {
    take(input, length)?;
    Ok(None)
}

fn take<'a>(input: &mut &'a [u8], length: usize) -> Result<&'a [u8], Error> {
    if input.len() < length {
        return Err(stream_error("truncated AWS event-stream header"));
    }
    let (value, rest) = input.split_at(length);
    *input = rest;
    Ok(value)
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn stream_error(message: impl Into<String>) -> Error {
    Error::InvalidResponse(message.into())
}

pub(super) async fn execute_chat_completions_provider_stream(
    request: ProviderChatCompletionsRequest,
) -> Result<ChatCompletionsStream, Error> {
    let body = serde_json::to_vec(&request.body).map_err(|error| {
        Error::InvalidRequest(format!(
            "failed to serialize chat completions request: {error}"
        ))
    })?;
    let client = http_client().clone();
    let mut builder = client.post(&request.url).body(body);
    for (name, value) in &request.upstream_headers {
        builder = builder.header(name, value);
    }
    if let Some(timeout) = request.timeout {
        builder = builder.timeout(timeout);
    }
    let wire = builder
        .build()
        .map_err(|error| Error::InvalidRequest(error.without_url().to_string()))?;
    let wire = request.auth.authenticate(wire).await.map_err(auth_error)?;
    let response = http_request(reqwest::RequestBuilder::from_parts(client, wire))
        .await
        .map_err(|error| {
            if error.is_connect() || error.is_builder() {
                Error::Connect(error.without_url().to_string())
            } else {
                Error::Network(error.without_url().to_string())
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .map_err(|error| Error::Network(error.without_url().to_string()))?;
        return Err(Error::Http {
            status: status.as_u16(),
            body: truncate_error_body(&body),
        });
    }

    let operation = request.plan.wire_operation();
    let transformation = request.plan.transformation();
    let (sender, receiver) = mpsc::channel(32);
    tokio::spawn(async move {
        let mut bytes = response.bytes_stream();
        let mut decoder = WireDecoder::new(operation);
        let mut state = ChatCompletionsStreamState::new(transformation);
        while let Some(chunk) = bytes.next().await {
            let wire_events = match chunk
                .map_err(|error| Error::Network(error.without_url().to_string()))
                .and_then(|chunk| decoder.push(&chunk))
            {
                Ok(events) => events,
                Err(error) => {
                    let _ = sender.send(Err(error)).await;
                    return;
                }
            };
            if !send_events(&sender, transformation, &mut state, wire_events).await {
                return;
            }
        }
        match decoder.finish() {
            Ok(events) => {
                let _ = send_events(&sender, transformation, &mut state, events).await;
            }
            Err(error) => {
                let _ = sender.send(Err(error)).await;
            }
        }
    });

    Ok(Box::pin(futures_util::stream::unfold(
        receiver,
        |mut receiver| async move { receiver.recv().await.map(|item| (item, receiver)) },
    )))
}

async fn send_events(
    sender: &mpsc::Sender<Result<ChatCompletionsStreamEvent, Error>>,
    transformation: ChatCompletionsTransformationKind,
    state: &mut ChatCompletionsStreamState,
    events: Vec<WireEvent>,
) -> bool {
    let mut output = VecDeque::new();
    for event in events {
        match transform_stream_event(transformation, state, event.name, event.data) {
            Ok(events) => output.extend(events.into_iter().map(Ok)),
            Err(error) => {
                output.push_back(Err(error));
                break;
            }
        }
    }
    while let Some(event) = output.pop_front() {
        if sender.send(event).await.is_err() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_decoder_handles_fragmented_frames() {
        let mut decoder = SseDecoder::default();
        assert!(
            decoder
                .push(b"event: content_block_delta\nda")
                .unwrap()
                .is_empty()
        );
        let events = decoder
            .push(b"ta: {\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n")
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "content_block_delta");
        assert_eq!(events[0].data["delta"]["text"], "hi");
    }

    #[test]
    fn aws_event_stream_decoder_checks_crc_and_fragmentation() {
        let frame = aws_frame(
            "contentBlockDelta",
            br#"{"contentBlockIndex":0,"delta":{"text":"hi"}}"#,
        );
        let mut decoder = AwsEventStreamDecoder::default();
        assert!(decoder.push(&frame[..7]).unwrap().is_empty());
        let events = decoder.push(&frame[7..]).unwrap();
        assert_eq!(events[0].name, "contentBlockDelta");
        assert_eq!(events[0].data["delta"]["text"], "hi");

        let mut corrupt = frame;
        corrupt[12] ^= 1;
        assert!(AwsEventStreamDecoder::default().push(&corrupt).is_err());
    }

    fn aws_frame(event: &str, payload: &[u8]) -> Vec<u8> {
        let mut headers = Vec::new();
        headers.push(11);
        headers.extend_from_slice(b":event-type");
        headers.push(7);
        headers.extend_from_slice(&(event.len() as u16).to_be_bytes());
        headers.extend_from_slice(event.as_bytes());
        let total = 16 + headers.len() + payload.len();
        let mut frame = Vec::new();
        frame.extend_from_slice(&(total as u32).to_be_bytes());
        frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
        frame.extend_from_slice(&crc32fast::hash(&frame).to_be_bytes());
        frame.extend_from_slice(&headers);
        frame.extend_from_slice(payload);
        frame.extend_from_slice(&crc32fast::hash(&frame).to_be_bytes());
        frame
    }
}
