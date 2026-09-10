mod transformation;
mod types;

pub(crate) use transformation::{
    transform_legacy_ocr_request, transform_ocr_response, transform_v3_ocr_request,
};
pub(crate) use types::{
    ReductoLegacyParams, ReductoLegacyRequest, ReductoResponse, ReductoUploadResponse,
    ReductoV3Params, ReductoV3Request,
};

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
pub(crate) struct ReductoV3Transformation;

impl OperationTransformation<OcrOperation> for ReductoV3Transformation {
    type WireOperation = OcrWireOperation;

    fn wire_operation(&self) -> Self::WireOperation {
        OcrWireOperation::ReductoParse
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Exact
    }
}

impl OcrTransformation for ReductoV3Transformation {
    type Params = ReductoV3Params;
    type WireRequest = ReductoV3Request;
    type WireResponse = ReductoResponse;
}

impl ParameterTransformation<OcrOperation> for ReductoV3Transformation {
    type Input = OcrParameterInput;
    type Output = ReductoV3Params;
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

impl RequestTransformation<OcrOperation> for ReductoV3Transformation {
    type Input = OcrTransformRequest<ReductoV3Params>;
    type Output = ReductoV3Request;
    type Error = OcrRequestError;

    fn transform_request(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        transform_v3_ocr_request(&input.model, input.document, &input.params)
    }
}

impl ResponseTransformation<OcrOperation> for ReductoV3Transformation {
    type Input = OcrTransformResponse<ReductoResponse>;
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReductoLegacyTransformation;

impl OperationTransformation<OcrOperation> for ReductoLegacyTransformation {
    type WireOperation = OcrWireOperation;

    fn wire_operation(&self) -> Self::WireOperation {
        OcrWireOperation::ReductoParseLegacy
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Exact
    }
}

impl OcrTransformation for ReductoLegacyTransformation {
    type Params = ReductoLegacyParams;
    type WireRequest = ReductoLegacyRequest;
    type WireResponse = ReductoResponse;
}

impl ParameterTransformation<OcrOperation> for ReductoLegacyTransformation {
    type Input = OcrParameterInput;
    type Output = ReductoLegacyParams;
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

impl RequestTransformation<OcrOperation> for ReductoLegacyTransformation {
    type Input = OcrTransformRequest<ReductoLegacyParams>;
    type Output = ReductoLegacyRequest;
    type Error = OcrRequestError;

    fn transform_request(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        transform_legacy_ocr_request(&input.model, input.document, &input.params)
    }
}

impl ResponseTransformation<OcrOperation> for ReductoLegacyTransformation {
    type Input = OcrTransformResponse<ReductoResponse>;
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
