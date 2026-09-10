mod params;
mod transformation;
mod types;

pub(crate) use params::{decode_input_params, map_ocr_params};
pub(crate) use transformation::{transform_ocr_request, transform_ocr_response};
pub(crate) use types::{
    AzureDocumentIntelligenceOperation, DocumentIntelligenceParams, DocumentIntelligenceRequest,
    OperationStatus,
};

use super::{DecodeOcrResponse, EncodeOcrRequest};
use crate::ocr::error::{OcrRequestError, OcrResponseError};
use crate::ocr::types::{LiteLLMOcrRequest, LiteLLMOcrResponse, OcrDocument};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DocumentIntelligenceCodec;

impl EncodeOcrRequest for DocumentIntelligenceCodec {
    type Params = DocumentIntelligenceParams;
    type WireRequest = DocumentIntelligenceRequest;

    #[tracing::instrument(
        name = "map_ocr_params",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    fn params(&self, request: &LiteLLMOcrRequest) -> Result<Self::Params, OcrRequestError> {
        map_ocr_params(
            decode_input_params(request.optional_params.clone(), "optional_params")?.into_known(),
        )
    }

    fn encode(
        &self,
        _model: &str,
        document: OcrDocument,
        _params: &Self::Params,
    ) -> Result<Self::WireRequest, OcrRequestError> {
        transform_ocr_request(document)
    }
}

impl DecodeOcrResponse for DocumentIntelligenceCodec {
    type WireResponse = AzureDocumentIntelligenceOperation;

    #[tracing::instrument(
        name = "transform_ocr_response",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    fn decode(
        &self,
        model: &str,
        response: Self::WireResponse,
    ) -> Result<LiteLLMOcrResponse, OcrResponseError> {
        transform_ocr_response(model, response)
    }
}
