mod transformation;
mod types;

pub(crate) use transformation::{
    transform_legacy_ocr_request, transform_ocr_response, transform_v3_ocr_request,
};
pub(crate) use types::{
    ReductoLegacyParams, ReductoLegacyRequest, ReductoResponse, ReductoUploadResponse,
    ReductoV3Params, ReductoV3Request,
};

use super::{DecodeOcrResponse, EncodeOcrRequest};
use crate::ocr::error::{OcrRequestError, OcrResponseError};
use crate::ocr::prepare::_prepare_ocr_request;
use crate::ocr::types::{LiteLLMOcrRequest, LiteLLMOcrResponse, OcrDocument};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReductoV3Codec;

impl EncodeOcrRequest for ReductoV3Codec {
    type Params = ReductoV3Params;
    type WireRequest = ReductoV3Request;

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
        transform_v3_ocr_request(model, document, params)
    }
}

impl DecodeOcrResponse for ReductoV3Codec {
    type WireResponse = ReductoResponse;

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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReductoLegacyCodec;

impl EncodeOcrRequest for ReductoLegacyCodec {
    type Params = ReductoLegacyParams;
    type WireRequest = ReductoLegacyRequest;

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
        transform_legacy_ocr_request(model, document, params)
    }
}

impl DecodeOcrResponse for ReductoLegacyCodec {
    type WireResponse = ReductoResponse;

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
