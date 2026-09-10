use crate::ocr::error::{OcrError, OcrRequestError};
use crate::ocr::types::LiteLLMOcrRequest;
use crate::url_utils::ApiUrl;

use super::ResolveOcrEndpoint;

const REDUCTO_API_BASE: &str = "https://platform.reducto.ai";

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReductoEndpoint;

impl<Params> ResolveOcrEndpoint<(), Params> for ReductoEndpoint {
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
        _params: &Params,
    ) -> Result<String, OcrError> {
        complete_url(request.connection.api_base.as_deref(), "parse")
    }
}

pub(crate) fn complete_url(api_base: Option<&str>, path: &str) -> Result<String, OcrError> {
    let base = api_base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(REDUCTO_API_BASE);
    ApiUrl::parse(base)
        .and_then(|url| url.complete_path(&[path]))
        .map(ApiUrl::into_string)
        .map_err(|_| OcrRequestError::RequestField {
            path: "api_base".into(),
        })
        .map_err(OcrError::from)
}
