mod transformation;
mod types;

pub(crate) use transformation::{transform_ocr_request, transform_ocr_response};
pub(crate) use types::{MistralOcrParams, MistralOcrRequest, MistralOcrResponse};

use super::{OcrParameterInput, OcrTransformRequest, OcrTransformResponse, OcrTransformation};
use crate::ocr::error::{OcrRequestError, OcrResponseError};
use crate::ocr::prepare::_prepare_ocr_request;
use crate::ocr::registry::OcrWireOperation;
use crate::ocr::types::{LiteLLMOcrResponse, OcrOperation};
use crate::operation::{
    Fidelity, OperationTransformation, ParameterTransformation, RequestTransformation,
    ResponseTransformation,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MistralOcrTransformation;

impl OperationTransformation<OcrOperation> for MistralOcrTransformation {
    type WireOperation = OcrWireOperation;

    fn wire_operation(&self) -> Self::WireOperation {
        OcrWireOperation::MistralOcr
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Exact
    }
}

impl OcrTransformation for MistralOcrTransformation {
    type Params = MistralOcrParams;
    type WireRequest = MistralOcrRequest;
    type WireResponse = MistralOcrResponse;
}

impl ParameterTransformation<OcrOperation> for MistralOcrTransformation {
    type Input = OcrParameterInput;
    type Output = MistralOcrParams;
    type Error = OcrRequestError;

    #[tracing::instrument(
        name = "map_ocr_params",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    fn transform_parameters(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        Ok(_prepare_ocr_request::<Self::Output>(input.optional_params)?.into_known())
    }
}

impl RequestTransformation<OcrOperation> for MistralOcrTransformation {
    type Input = OcrTransformRequest<MistralOcrParams>;
    type Output = MistralOcrRequest;
    type Error = OcrRequestError;

    fn transform_request(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        transform_ocr_request(&input.model, input.document, &input.params)
    }
}

impl ResponseTransformation<OcrOperation> for MistralOcrTransformation {
    type Input = OcrTransformResponse<MistralOcrResponse>;
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
