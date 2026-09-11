use litellm_auth::SecretValue;
use litellm_operation::Operation;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ocr;

impl Operation for Ocr {
    type Request<'a> = OcrRequest<'a>;
    type Response = OcrResponse;

    const NAME: &'static str = "ocr";
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OcrRequest<'a> {
    pub model: &'a str,
    pub document_uri: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OcrResponse {
    pub pages: Vec<OcrPage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OcrPage {
    pub index: u32,
    pub markdown: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OcrCall {
    pub model: String,
    pub document: OcrDocument,
    pub parameters: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OcrDocument {
    Uri(String),
    Inline { media_type: String, bytes: Vec<u8> },
}

impl OcrDocument {
    pub fn uri(&self) -> Option<&str> {
        match self {
            Self::Uri(uri) => Some(uri),
            Self::Inline { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OcrCallContext {
    pub api_key: Option<SecretValue>,
    pub api_base: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_reports_uri_only_for_remote_sources() {
        assert_eq!(
            OcrDocument::Uri("https://example.com/doc.pdf".into()).uri(),
            Some("https://example.com/doc.pdf")
        );
        assert_eq!(
            OcrDocument::Inline {
                media_type: "application/pdf".into(),
                bytes: vec![1, 2, 3],
            }
            .uri(),
            None
        );
    }
}
