pub(crate) mod document_intelligence;
pub(crate) mod mistral;
pub(crate) mod reducto;

use serde::{Serialize, de::DeserializeOwned};

use super::error::{OcrRequestError, OcrResponseError};
use super::types::{LiteLLMOcrRequest, LiteLLMOcrResponse, OcrDocument};

pub(crate) trait EncodeOcrRequest: Send + Sync {
    type Params: Send + Sync;
    type WireRequest: Serialize + DeserializeOwned + Send + Sync;

    fn params(&self, request: &LiteLLMOcrRequest) -> Result<Self::Params, OcrRequestError>;

    fn encode(
        &self,
        model: &str,
        document: OcrDocument,
        params: &Self::Params,
    ) -> Result<Self::WireRequest, OcrRequestError>;
}

pub(crate) trait DecodeOcrResponse: Send + Sync {
    type WireResponse: DeserializeOwned + Send;

    fn decode(
        &self,
        model: &str,
        response: Self::WireResponse,
    ) -> Result<LiteLLMOcrResponse, OcrResponseError>;
}
