use std::sync::{Arc, Mutex};

use litellm_auth::SecretValue;
use litellm_operation_ocr::{OcrCall, OcrDocument, OcrResponseFormat};
use litellm_providers::mistral::ocr::{MistralOcrConfig, MistralOcrContext};
use litellm_transport::{Authenticated, FinalRequest, Transport};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq)]
struct CapturedRequest {
    url: String,
    authorization: Option<String>,
    trace: Option<String>,
    content_type: Option<String>,
    body: Value,
}

#[derive(Clone)]
struct RecordingTransport {
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    response: Value,
}

impl Transport for RecordingTransport {
    async fn send(
        &self,
        request: FinalRequest<Authenticated>,
    ) -> Result<reqwest::Response, litellm_transport::Error> {
        let request = request.request();
        let header = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        let body = request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
            .expect("request has a JSON body");
        self.captured.lock().unwrap().push(CapturedRequest {
            url: request.url().to_string(),
            authorization: header("authorization"),
            trace: header("x-trace"),
            content_type: header("content-type"),
            body,
        });
        Ok(reqwest::Response::from(http::Response::new(
            serde_json::to_vec(&self.response).expect("fixture serializes"),
        )))
    }
}

#[tokio::test]
async fn direct_mistral_composition_maps_authenticates_executes_and_adapts() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let provider_response = json!({
        "pages": [{
            "index": 0,
            "markdown": "hello",
            "blocks": [{"type": "text", "content": "hello"}]
        }],
        "usage_info": {"pages_processed": 1, "doc_size_bytes": 3},
        "provider_only": "preserved"
    });
    let config = MistralOcrConfig::new(RecordingTransport {
        captured: captured.clone(),
        response: provider_response.clone(),
    })
    .expect("environment compiles");
    let call = OcrCall {
        model: "mistral-ocr-latest".into(),
        document: OcrDocument::DocumentUrl {
            document_url: "data:application/pdf;base64,YWJj".into(),
            extra_fields: Map::from_iter([("document_name".into(), json!("report.pdf"))]),
        },
        parameters: Map::from_iter([
            ("extract_header".into(), json!(true)),
            ("pages".into(), json!("0-2")),
            ("unknown".into(), json!("ignored")),
        ]),
        response_format: OcrResponseFormat::Native,
    };
    let context = MistralOcrContext {
        api_key: Some(SecretValue::new("test-key")),
        api_base: Some("https://provider.test/v1?tenant=a".into()),
        extra_headers: vec![("x-trace".into(), "trace-1".into())],
    };

    let response = config
        .execute(call, &context)
        .await
        .expect("composition succeeds");

    assert_eq!(response.model, "mistral-ocr-latest");
    assert_eq!(response.object, "ocr");
    assert_eq!(response.pages[0]["blocks"][0]["content"], "hello");
    assert_eq!(response.extra_fields["provider_only"], "preserved");
    assert_eq!(response.provider_native_response, Some(provider_response));
    assert_eq!(
        *captured.lock().unwrap(),
        vec![CapturedRequest {
            url: "https://provider.test/v1/ocr?tenant=a".into(),
            authorization: Some("Bearer test-key".into()),
            trace: Some("trace-1".into()),
            content_type: Some("application/json".into()),
            body: json!({
                "model": "mistral-ocr-latest",
                "document": {
                    "type": "document_url",
                    "document_url": "data:application/pdf;base64,YWJj",
                    "document_name": "report.pdf"
                },
                "pages": "0-2",
                "extract_header": true
            }),
        }]
    );
}

#[tokio::test]
async fn caller_authorization_is_preserved_without_an_api_key() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let config = MistralOcrConfig::new(RecordingTransport {
        captured: captured.clone(),
        response: json!({"pages": []}),
    })
    .expect("environment compiles");
    let call = OcrCall {
        model: "mistral-ocr-latest".into(),
        document: OcrDocument::ImageUrl {
            image_url: "https://example.test/image.png".into(),
            extra_fields: Map::new(),
        },
        parameters: Map::new(),
        response_format: OcrResponseFormat::Litellm,
    };
    let context = MistralOcrContext {
        api_key: None,
        api_base: Some("https://provider.test/v1/ocr".into()),
        extra_headers: vec![("Authorization".into(), "Bearer caller-key".into())],
    };

    config
        .execute(call, &context)
        .await
        .expect("caller credential succeeds");

    assert_eq!(
        captured.lock().unwrap()[0].authorization.as_deref(),
        Some("Bearer caller-key")
    );
}
