use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MistralOcrParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<MistralOcrPages>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_image_base64: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_min_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox_annotation_format: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_annotation_format: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_annotation_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_header: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_footer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_scores_granularity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_blocks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MistralOcrPages {
    Indices(Vec<i64>),
    Range(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MistralOcrRequest<Document> {
    pub model: String,
    pub document: Document,
    #[serde(flatten)]
    pub params: MistralOcrParams,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MistralOcrResponse {
    #[serde(default)]
    pub pages: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_annotation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_info: Option<Value>,
    #[serde(flatten)]
    pub extra_fields: Map<String, Value>,
}
