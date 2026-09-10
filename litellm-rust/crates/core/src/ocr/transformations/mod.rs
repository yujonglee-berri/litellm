pub(crate) mod document_intelligence;
pub(crate) mod mistral;
pub(crate) mod reducto;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use super::error::{OcrRequestError, OcrResponseError};
use super::registry::OcrWireOperation;
use super::types::{OcrDocument, OcrOperation};
use crate::operation::{
    OperationTransformation, ParameterTransformation, RequestTransformation, ResponseTransformation,
};

pub(crate) struct OcrParameterInput {
    pub(crate) optional_params: Map<String, Value>,
}

pub(crate) struct OcrTransformRequest<P> {
    pub(crate) model: String,
    pub(crate) document: OcrDocument,
    pub(crate) params: P,
}

pub(crate) struct OcrTransformResponse<R> {
    pub(crate) model: String,
    pub(crate) response: R,
}

pub(crate) trait OcrTransformation:
    OperationTransformation<OcrOperation, WireOperation = OcrWireOperation>
    + ParameterTransformation<
        OcrOperation,
        Input = OcrParameterInput,
        Output = Self::Params,
        Error = OcrRequestError,
    > + RequestTransformation<
        OcrOperation,
        Input = OcrTransformRequest<Self::Params>,
        Output = Self::WireRequest,
        Error = OcrRequestError,
    > + ResponseTransformation<
        OcrOperation,
        Input = OcrTransformResponse<Self::WireResponse>,
        Error = OcrResponseError,
    > + Send
    + Sync
{
    type Params: Clone + Send + Sync;
    type WireRequest: Serialize + DeserializeOwned + Send + Sync;
    type WireResponse: DeserializeOwned + Send;
}
