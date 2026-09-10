//! Opt-in live smoke tests for every provider/operation plan the Rust core can execute.
//!
//! Run the configured cases with:
//! `cargo test -p litellm-core --features bedrock-auth provider_operation_smoke_tests -- --ignored --nocapture`
//!
//! Every case skips itself when its listed environment variables are absent, so credentials for
//! providers can be added independently. Secret values are never printed.

use std::env;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Map, Value, json};

use crate::chat_completions::types::{ChatCompletionsRequest, ChatCompletionsStreamEvent};
use crate::chat_completions::{chat_completions, chat_completions_stream};
use crate::ocr::{LiteLLMOcrRequest, OcrDocument, ocr};

const TIMEOUT: Duration = Duration::from_secs(90);
const DOCUMENT_URL_ENV: &str = "LITELLM_SMOKE_OCR_DOCUMENT_URL";

fn configured(names: &[&str]) -> bool {
    let missing = names
        .iter()
        .filter(|name| env::var(name).map_or(true, |value| value.trim().is_empty()))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        eprintln!("skipped: missing {}", missing.join(", "));
        return false;
    }
    true
}

async fn smoke_chat_stream(provider: &str, model: &str, optional_params: Map<String, Value>) {
    let mut stream = chat_completions_stream(ChatCompletionsRequest {
        model,
        messages: json!([{"role": "user", "content": "Reply with exactly OK."}]),
        optional_params,
        api_key: None,
        api_base: None,
        custom_llm_provider: Some(provider),
        extra_headers: None,
        timeout: Some(TIMEOUT),
    })
    .await
    .expect("live chat completion stream should connect");

    let mut text = String::new();
    let mut finished = false;
    while let Some(event) = stream.next().await {
        match event.expect("live stream event should decode") {
            ChatCompletionsStreamEvent::TextDelta { text: delta, .. } => text.push_str(&delta),
            ChatCompletionsStreamEvent::Finish { .. } => finished = true,
            ChatCompletionsStreamEvent::Error { message } => panic!("provider stream: {message}"),
            _ => {}
        }
    }
    assert!(
        !text.trim().is_empty(),
        "provider returned no streamed text"
    );
    assert!(finished, "provider stream returned no finish event");
}

fn setting(name: &str) -> String {
    env::var(name).expect("environment checked above")
}

fn object(value: Value) -> Map<String, Value> {
    value
        .as_object()
        .expect("test request params must be an object")
        .clone()
}

async fn smoke_chat(provider: &str, model: &str, optional_params: Map<String, Value>) {
    let response = chat_completions(ChatCompletionsRequest {
        model,
        messages: json!([{"role": "user", "content": "Reply with exactly OK."}]),
        optional_params,
        api_key: None,
        api_base: None,
        custom_llm_provider: Some(provider),
        extra_headers: None,
        timeout: Some(TIMEOUT),
    })
    .await
    .expect("live chat completion should succeed");

    assert_eq!(response.choices.len(), 1);
    assert!(
        response.choices[0]
            .message
            .content
            .as_deref()
            .is_some_and(|content| !content.trim().is_empty()),
        "provider returned no assistant text"
    );
    assert!(!response.model.trim().is_empty());
}

async fn smoke_ocr(provider: &str, model: String, api_base: Option<String>, document_url: String) {
    let request = LiteLLMOcrRequest::new(
        model,
        OcrDocument::DocumentUrl {
            document_url,
            extra_fields: Map::new(),
        },
        Some(provider),
        Map::new(),
    )
    .expect("smoke case should resolve to an OCR plan");
    let connection = crate::ocr::OcrConnection {
        api_base,
        timeout: TIMEOUT,
        poll_timeout: TIMEOUT,
        ..request.connection.clone()
    };
    let request = LiteLLMOcrRequest {
        connection,
        ..request
    };
    let response = ocr(request).await.expect("live OCR request should succeed");

    assert_eq!(response.object, "ocr");
    assert!(!response.model.trim().is_empty());
    assert!(!response.pages.is_empty(), "provider returned no OCR pages");
}

#[tokio::test]
#[ignore = "requires live Anthropic credentials"]
async fn anthropic_messages_chat_completion() {
    if !configured(&["ANTHROPIC_API_KEY", "LITELLM_SMOKE_ANTHROPIC_MODEL"]) {
        return;
    }
    let model = setting("LITELLM_SMOKE_ANTHROPIC_MODEL");
    smoke_chat(
        "anthropic",
        &model,
        object(json!({"max_tokens": 8, "temperature": 0})),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires live Anthropic credentials"]
async fn anthropic_messages_chat_completion_stream() {
    if !configured(&["ANTHROPIC_API_KEY", "LITELLM_SMOKE_ANTHROPIC_MODEL"]) {
        return;
    }
    let model = setting("LITELLM_SMOKE_ANTHROPIC_MODEL");
    smoke_chat_stream(
        "anthropic",
        &model,
        object(json!({"max_tokens": 8, "temperature": 0})),
    )
    .await;
}

#[cfg(feature = "bedrock-auth")]
#[tokio::test]
#[ignore = "requires live Bedrock credentials"]
async fn bedrock_converse_chat_completion() {
    if !configured(&["AWS_BEARER_TOKEN_BEDROCK", "LITELLM_SMOKE_BEDROCK_MODEL"]) {
        return;
    }
    let model = setting("LITELLM_SMOKE_BEDROCK_MODEL");
    smoke_chat(
        "bedrock",
        &model,
        object(json!({"maxTokens": 8, "temperature": 0})),
    )
    .await;
}

#[cfg(feature = "bedrock-auth")]
#[tokio::test]
#[ignore = "requires live Bedrock credentials"]
async fn bedrock_converse_chat_completion_stream() {
    if !configured(&["AWS_BEARER_TOKEN_BEDROCK", "LITELLM_SMOKE_BEDROCK_MODEL"]) {
        return;
    }
    let model = setting("LITELLM_SMOKE_BEDROCK_MODEL");
    smoke_chat_stream(
        "bedrock",
        &model,
        object(json!({"maxTokens": 8, "temperature": 0})),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires live Mistral OCR credentials"]
async fn mistral_ocr() {
    if !configured(&["MISTRAL_API_KEY", DOCUMENT_URL_ENV]) {
        return;
    }
    smoke_ocr(
        "mistral",
        "mistral-ocr-latest".into(),
        None,
        setting(DOCUMENT_URL_ENV),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires live Azure AI OCR credentials"]
async fn azure_ai_mistral_ocr() {
    if !configured(&["AZURE_AI_API_KEY", "AZURE_AI_API_BASE", DOCUMENT_URL_ENV]) {
        return;
    }
    smoke_ocr(
        "azure_ai",
        "mistral-ocr-latest".into(),
        Some(setting("AZURE_AI_API_BASE")),
        setting(DOCUMENT_URL_ENV),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires live Azure Document Intelligence credentials"]
async fn azure_document_intelligence_analyze() {
    if !configured(&[
        "AZURE_DOCUMENT_INTELLIGENCE_API_KEY",
        "AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT",
        "LITELLM_SMOKE_AZURE_DOCUMENT_INTELLIGENCE_MODEL",
        DOCUMENT_URL_ENV,
    ]) {
        return;
    }
    smoke_ocr(
        "azure_ai",
        format!(
            "doc-intelligence/{}",
            setting("LITELLM_SMOKE_AZURE_DOCUMENT_INTELLIGENCE_MODEL")
        ),
        Some(setting("AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT")),
        setting(DOCUMENT_URL_ENV),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires live Reducto credentials"]
async fn reducto_parse_legacy() {
    if !configured(&["REDUCTO_API_KEY", DOCUMENT_URL_ENV]) {
        return;
    }
    smoke_ocr(
        "reducto",
        "parse-legacy".into(),
        None,
        setting(DOCUMENT_URL_ENV),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires live Reducto credentials"]
async fn reducto_parse_v3() {
    if !configured(&["REDUCTO_API_KEY", DOCUMENT_URL_ENV]) {
        return;
    }
    smoke_ocr("reducto", "parse".into(), None, setting(DOCUMENT_URL_ENV)).await;
}
