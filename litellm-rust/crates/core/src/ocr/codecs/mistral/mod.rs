mod transformation;
mod types;

pub(crate) use transformation::{transform_ocr_request, transform_ocr_response};
pub(crate) use types::{MistralOcrParams, MistralOcrRequest, MistralOcrResponse};

use super::{DecodeOcrResponse, EncodeOcrRequest};
use crate::ocr::error::{OcrRequestError, OcrResponseError};
use crate::ocr::prepare::_prepare_ocr_request;
use crate::ocr::types::{LiteLLMOcrRequest, LiteLLMOcrResponse, OcrDocument};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MistralOcrCodec;

impl EncodeOcrRequest for MistralOcrCodec {
    type Params = MistralOcrParams;
    type WireRequest = MistralOcrRequest;

    #[tracing::instrument(
        name = "map_ocr_params",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    fn params(&self, request: &LiteLLMOcrRequest) -> Result<Self::Params, OcrRequestError> {
        Ok(_prepare_ocr_request::<Self::Params>(request)?.into_known())
    }

    fn encode(
        &self,
        model: &str,
        document: OcrDocument,
        params: &Self::Params,
    ) -> Result<Self::WireRequest, OcrRequestError> {
        transform_ocr_request(model, document, params)
    }
}

impl DecodeOcrResponse for MistralOcrCodec {
    type WireResponse = MistralOcrResponse;

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
