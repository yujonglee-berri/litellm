use litellm_operation::{Delivery, Error, OperationCodec};
use serde::{Deserialize, Serialize};

use crate::plan::{
    DocumentIntelligenceTransformation, MistralTransformation, ReductoLegacyTransformation,
    ReductoV3Transformation,
};
use crate::types::{OcrCall, OcrDocument, OcrPage, OcrResponse};

fn string_parameter(call: &OcrCall, name: &'static str) -> Result<Option<String>, Error> {
    match call.parameters.get(name) {
        None => Ok(None),
        Some(value) => match value.as_str() {
            Some(value) => Ok(Some(value.to_string())),
            None => Err(Error::InvalidRequest(format!(
                "ocr parameter {name} must be a string"
            ))),
        },
    }
}

fn document_uri(document: &OcrDocument, provider: &'static str) -> Result<String, Error> {
    document
        .uri()
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidRequest(format!("{provider} ocr requires a document url")))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MistralParams {
    pub pages: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MistralOcrRequest {
    pub model: String,
    pub document_url: String,
    pub pages: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MistralOcrResponse {
    pub pages: Vec<MistralOcrPage>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MistralOcrPage {
    pub index: u32,
    pub markdown: String,
}

impl OperationCodec for MistralTransformation {
    type Call = OcrCall;
    type Context = crate::types::OcrCallContext;
    type Params = MistralParams;
    type WireRequest = MistralOcrRequest;
    type WireResponse = MistralOcrResponse;
    type Response = OcrResponse;

    fn params(&self, call: &OcrCall) -> Result<MistralParams, Error> {
        Ok(MistralParams {
            pages: string_parameter(call, "pages")?,
        })
    }

    fn encode(
        &self,
        call: &OcrCall,
        params: &MistralParams,
        _delivery: Delivery,
    ) -> Result<MistralOcrRequest, Error> {
        Ok(MistralOcrRequest {
            model: call.model.clone(),
            document_url: document_uri(&call.document, "mistral")?,
            pages: params.pages.clone(),
        })
    }

    fn decode(&self, _call: &OcrCall, response: MistralOcrResponse) -> Result<OcrResponse, Error> {
        Ok(OcrResponse {
            pages: response
                .pages
                .into_iter()
                .map(|page| OcrPage {
                    index: page.index,
                    markdown: page.markdown,
                })
                .collect(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentIntelligenceParams {
    pub pages: Option<String>,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentIntelligenceRequest {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentIntelligenceResponse {
    pub content: String,
}

impl OperationCodec for DocumentIntelligenceTransformation {
    type Call = OcrCall;
    type Context = crate::types::OcrCallContext;
    type Params = DocumentIntelligenceParams;
    type WireRequest = DocumentIntelligenceRequest;
    type WireResponse = DocumentIntelligenceResponse;
    type Response = OcrResponse;

    fn params(&self, call: &OcrCall) -> Result<DocumentIntelligenceParams, Error> {
        Ok(DocumentIntelligenceParams {
            pages: string_parameter(call, "pages")?,
            features: Vec::new(),
        })
    }

    fn encode(
        &self,
        _call: &OcrCall,
        _params: &DocumentIntelligenceParams,
        _delivery: Delivery,
    ) -> Result<DocumentIntelligenceRequest, Error> {
        Ok(DocumentIntelligenceRequest {})
    }

    fn decode(
        &self,
        _call: &OcrCall,
        response: DocumentIntelligenceResponse,
    ) -> Result<OcrResponse, Error> {
        Ok(OcrResponse {
            pages: vec![OcrPage {
                index: 0,
                markdown: response.content,
            }],
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReductoParseRequest {
    pub document_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReductoParseResponse {
    pub content: String,
}

impl OperationCodec for ReductoV3Transformation {
    type Call = OcrCall;
    type Context = crate::types::OcrCallContext;
    type Params = ();
    type WireRequest = ReductoParseRequest;
    type WireResponse = ReductoParseResponse;
    type Response = OcrResponse;

    fn params(&self, _call: &OcrCall) -> Result<(), Error> {
        Ok(())
    }

    fn encode(
        &self,
        call: &OcrCall,
        _params: &(),
        _delivery: Delivery,
    ) -> Result<ReductoParseRequest, Error> {
        Ok(ReductoParseRequest {
            document_url: document_uri(&call.document, "reducto")?,
        })
    }

    fn decode(
        &self,
        _call: &OcrCall,
        response: ReductoParseResponse,
    ) -> Result<OcrResponse, Error> {
        Ok(OcrResponse {
            pages: vec![OcrPage {
                index: 0,
                markdown: response.content,
            }],
        })
    }
}

impl OperationCodec for ReductoLegacyTransformation {
    type Call = OcrCall;
    type Context = crate::types::OcrCallContext;
    type Params = ();
    type WireRequest = ReductoParseRequest;
    type WireResponse = ReductoParseResponse;
    type Response = OcrResponse;

    fn params(&self, _call: &OcrCall) -> Result<(), Error> {
        Ok(())
    }

    fn encode(
        &self,
        call: &OcrCall,
        _params: &(),
        _delivery: Delivery,
    ) -> Result<ReductoParseRequest, Error> {
        Ok(ReductoParseRequest {
            document_url: document_uri(&call.document, "reducto")?,
        })
    }

    fn decode(
        &self,
        _call: &OcrCall,
        response: ReductoParseResponse,
    ) -> Result<OcrResponse, Error> {
        Ok(OcrResponse {
            pages: vec![OcrPage {
                index: 0,
                markdown: response.content,
            }],
        })
    }
}
