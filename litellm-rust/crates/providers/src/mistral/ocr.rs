use litellm_adapters::ocr::MistralOcrAdapter;
use litellm_auth::{ApiKeyAuth, ApiKeySource, SecretValue};
use litellm_operation::{CompleteHooks, NoHooks};
use litellm_operation_ocr::{OcrCall, OcrResponse};
use litellm_pipeline::{Error, JsonExecution, Pipeline, ResolveEndpoint};
use litellm_protocol::mistral::ocr::MistralOcrParams;
use litellm_transport::Transport;
use url::Url;

const MISTRAL_OCR_API_BASE: &str = "https://api.mistral.ai/v1";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MistralOcrContext {
    pub api_key: Option<SecretValue>,
    pub api_base: Option<String>,
    pub extra_headers: Vec<(String, String)>,
}

impl ApiKeySource for MistralOcrContext {
    fn api_key(&self) -> Option<&SecretValue> {
        self.api_key.as_ref()
    }

    fn extra_headers(&self) -> &[(String, String)] {
        &self.extra_headers
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MistralOcrEndpoint;

impl ResolveEndpoint<(), MistralOcrParams, OcrCall, MistralOcrContext> for MistralOcrEndpoint {
    fn resolve(
        &self,
        _call: &OcrCall,
        context: &MistralOcrContext,
        _auth: &(),
        _params: &MistralOcrParams,
    ) -> Result<String, Error> {
        let base = context
            .api_base
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(MISTRAL_OCR_API_BASE);
        complete_path(base, &["v1", "ocr"])
    }
}

fn complete_path(base: &str, target: &[&str]) -> Result<String, Error> {
    let mut url = Url::parse(base.trim())
        .map_err(|error| Error::InvalidRequest(format!("api_base is invalid: {error}")))?;
    let existing = url
        .path_segments()
        .ok_or_else(|| Error::InvalidRequest("api_base cannot be used as a base".into()))?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let overlap = (0..=existing.len().min(target.len()))
        .rev()
        .find(|length| existing[existing.len() - length..] == target[..*length])
        .unwrap_or(0);
    url.path_segments_mut()
        .map_err(|()| Error::InvalidRequest("api_base cannot be used as a base".into()))?
        .pop_if_empty()
        .extend(target[overlap..].iter().copied());
    Ok(url.into())
}

pub struct MistralOcrConfig<T, H = NoHooks> {
    pipeline: Pipeline<ApiKeyAuth, MistralOcrEndpoint, MistralOcrAdapter, JsonExecution<T>, H>,
}

impl<T> MistralOcrConfig<T, NoHooks> {
    pub fn new(transport: T) -> Self {
        Self::with_hooks(transport, NoHooks)
    }
}

impl<T, H> MistralOcrConfig<T, H> {
    pub fn with_hooks(transport: T, hooks: H) -> Self {
        Self {
            pipeline: Pipeline::new(
                ApiKeyAuth::bearer("Mistral", Some("MISTRAL_API_KEY")),
                MistralOcrEndpoint,
                MistralOcrAdapter,
                JsonExecution::new(transport),
                hooks,
            ),
        }
    }
}

impl<T, H> MistralOcrConfig<T, H>
where
    T: Transport + Send + Sync,
    H: CompleteHooks<OcrCall, OcrResponse>,
{
    pub async fn execute(
        &self,
        call: OcrCall,
        context: &MistralOcrContext,
    ) -> Result<OcrResponse, Error> {
        self.pipeline.handle(call, context).await
    }
}
