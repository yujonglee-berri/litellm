use crate::constants::MISTRAL_OCR_API_BASE;
use crate::ocr::error::{OcrError, OcrRequestError};
use crate::ocr::transformations::mistral::MistralOcrParams;
use crate::ocr::types::LiteLLMOcrRequest;
use crate::url_utils::ApiUrl;

use super::ResolveOcrEndpoint;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MistralEndpoint;

impl ResolveOcrEndpoint<(), MistralOcrParams> for MistralEndpoint {
    #[tracing::instrument(
        name = "complete_url",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    fn resolve(
        &self,
        request: &LiteLLMOcrRequest,
        _auth: &(),
        _params: &MistralOcrParams,
    ) -> Result<String, OcrError> {
        let base = request
            .connection
            .api_base
            .as_deref()
            .map(str::trim)
            .filter(|base| !base.is_empty())
            .unwrap_or(MISTRAL_OCR_API_BASE);
        ApiUrl::parse(base)
            .and_then(|url| url.complete_path(&["v1", "ocr"]))
            .map(ApiUrl::into_string)
            .map_err(|_| OcrRequestError::RequestField {
                path: "api_base".into(),
            })
            .map_err(OcrError::from)
    }
}
