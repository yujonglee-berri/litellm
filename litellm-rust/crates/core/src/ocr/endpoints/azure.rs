use crate::Error;
use crate::constants::{AZURE_AI_OCR_PATH, AZURE_DI_API_VERSION};
use crate::ocr::codecs::document_intelligence::DocumentIntelligenceParams;
use crate::ocr::codecs::mistral::MistralOcrParams;
use crate::ocr::error::{OcrError, OcrRequestError};
use crate::ocr::prepare::credential_env;
use crate::ocr::types::LiteLLMOcrRequest;
use crate::url_utils::ApiUrl;

use super::ResolveOcrEndpoint;

const AZURE_AI_API_BASE_ENV: &str = "AZURE_AI_API_BASE";
const AZURE_DI_ENDPOINT_ENV: &str = "AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT";

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AzureMistralEndpoint;

impl ResolveOcrEndpoint<(), MistralOcrParams> for AzureMistralEndpoint {
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
        let base = nonblank(request.connection.api_base.clone())
            .or_else(|| nonblank(credential_env(AZURE_AI_API_BASE_ENV)))
            .ok_or_else(|| {
                Error::Auth(
                    "Missing Azure AI API Base - Set AZURE_AI_API_BASE environment variable or pass api_base parameter"
                        .into(),
                )
            })?;
        let path: Vec<&str> = AZURE_AI_OCR_PATH.trim_matches('/').split('/').collect();
        ApiUrl::parse(&base)
            .and_then(|url| url.complete_path(&path))
            .map(ApiUrl::into_string)
            .map_err(|_| OcrRequestError::RequestField {
                path: "api_base".into(),
            })
            .map_err(OcrError::from)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AzureDocumentIntelligenceEndpoint;

impl ResolveOcrEndpoint<(), DocumentIntelligenceParams> for AzureDocumentIntelligenceEndpoint {
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
        params: &DocumentIntelligenceParams,
    ) -> Result<String, OcrError> {
        let endpoint = nonblank(request.connection.api_base.clone())
            .or_else(|| nonblank(credential_env(AZURE_DI_ENDPOINT_ENV)))
            .ok_or_else(|| {
                Error::Auth(
                    "Missing Azure Document Intelligence API Base - Set AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT or pass api_base"
                        .into(),
                )
            })?;
        let model = format!("{}:analyze", model_id(&request.model)?);
        ApiUrl::parse(&endpoint)
            .and_then(|url| url.complete_path(&["documentintelligence", "documentModels", &model]))
            .map(|url| {
                url.append_query_pairs(
                    [("api-version", AZURE_DI_API_VERSION)]
                        .into_iter()
                        .chain(params.pages.iter().map(|pages| ("pages", pages.as_str())))
                        .chain(
                            params
                                .features
                                .iter()
                                .map(|features| ("features", features.as_str())),
                        ),
                )
                .into_string()
            })
            .map_err(|_| OcrRequestError::RequestField {
                path: "api_base".into(),
            })
            .map_err(OcrError::from)
    }
}

fn model_id(model: &str) -> Result<&str, OcrRequestError> {
    let model = model.rsplit('/').next().unwrap_or(model);
    if matches!(model, "." | "..") {
        return Err(OcrRequestError::DotModel);
    }
    Ok(model)
}

fn nonblank(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
