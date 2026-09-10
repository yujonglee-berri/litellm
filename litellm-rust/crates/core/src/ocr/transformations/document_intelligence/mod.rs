mod params;
mod transformation;
mod types;

pub(crate) use params::{decode_input_params, map_ocr_params};
pub(crate) use transformation::{transform_ocr_request, transform_ocr_response};
pub(crate) use types::{
    AzureDocumentIntelligenceOperation, DocumentIntelligenceParams, DocumentIntelligenceRequest,
    OperationStatus,
};

use super::{OcrParameterInput, OcrTransformRequest, OcrTransformResponse, OcrTransformation};
use crate::ocr::error::{OcrRequestError, OcrResponseError};
use crate::ocr::registry::OcrWireOperation;
use crate::ocr::types::{LiteLLMOcrResponse, OcrOperation};
use crate::operation::{
    Fidelity, OperationTransformation, ParameterTransformation, RequestTransformation,
    ResponseTransformation,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DocumentIntelligenceTransformation;

impl OperationTransformation<OcrOperation> for DocumentIntelligenceTransformation {
    type WireOperation = OcrWireOperation;

    fn wire_operation(&self) -> Self::WireOperation {
        OcrWireOperation::AzureDocumentIntelligenceAnalyze
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Exact
    }
}

impl OcrTransformation for DocumentIntelligenceTransformation {
    type Params = DocumentIntelligenceParams;
    type WireRequest = DocumentIntelligenceRequest;
    type WireResponse = AzureDocumentIntelligenceOperation;
}

impl ParameterTransformation<OcrOperation> for DocumentIntelligenceTransformation {
    type Input = OcrParameterInput;
    type Output = DocumentIntelligenceParams;
    type Error = OcrRequestError;

    #[tracing::instrument(
        name = "map_ocr_params",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    fn transform_parameters(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        map_ocr_params(decode_input_params(input.optional_params, "optional_params")?.into_known())
    }
}

impl RequestTransformation<OcrOperation> for DocumentIntelligenceTransformation {
    type Input = OcrTransformRequest<DocumentIntelligenceParams>;
    type Output = DocumentIntelligenceRequest;
    type Error = OcrRequestError;

    fn transform_request(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        transform_ocr_request(input.document)
    }
}

impl ResponseTransformation<OcrOperation> for DocumentIntelligenceTransformation {
    type Input = OcrTransformResponse<AzureDocumentIntelligenceOperation>;
    type Error = OcrResponseError;

    #[tracing::instrument(
        name = "transform_ocr_response",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    fn transform_response(&self, input: Self::Input) -> Result<LiteLLMOcrResponse, Self::Error> {
        transform_ocr_response(&input.model, input.response)
    }
}
