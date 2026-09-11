use litellm_operation::Operation;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ocr;

impl Operation for Ocr {
    type Request<'a> = OcrRequest<'a>;
    type Response = OcrResponse;

    const NAME: &'static str = "ocr";
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OcrRequest<'a> {
    pub model: &'a str,
    pub document: &'a OcrDocument,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrResponse {
    pub pages: Vec<Value>,
    pub model: String,
    pub document_annotation: Option<Value>,
    pub usage_info: Option<Value>,
    pub object: String,
    #[serde(flatten)]
    pub extra_fields: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_native_response: Option<Value>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OcrResponseFormat {
    #[default]
    Litellm,
    Native,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OcrCall {
    pub model: String,
    pub document: OcrDocument,
    pub parameters: Map<String, Value>,
    pub response_format: OcrResponseFormat,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OcrDocument {
    #[serde(rename = "document_url")]
    DocumentUrl {
        document_url: String,
        #[serde(flatten)]
        extra_fields: Map<String, Value>,
    },
    #[serde(rename = "image_url")]
    ImageUrl {
        image_url: String,
        #[serde(flatten)]
        extra_fields: Map<String, Value>,
    },
}
