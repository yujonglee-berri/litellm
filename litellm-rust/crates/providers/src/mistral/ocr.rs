use litellm_adapters::ocr::MistralOcrAdapter;
use litellm_auth::{
    ApiKeyAuth, ApiKeyAuthContext, ApiKeySource, AuthConfigurationError, AuthError,
    CredentialLocation, CredentialPlan, CredentialRef, ExistingCredentialPolicy, SecretValue,
};
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

impl ResolveEndpoint<ApiKeyAuthContext, MistralOcrParams, OcrCall, MistralOcrContext>
    for MistralOcrEndpoint
{
    fn resolve(
        &self,
        _call: &OcrCall,
        context: &MistralOcrContext,
        _auth: &ApiKeyAuthContext,
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
    pub fn new(transport: T) -> Result<Self, AuthError> {
        Self::with_environment_lookup(transport, environment_lookup)
    }

    pub fn with_environment_lookup<E>(transport: T, environment: E) -> Result<Self, AuthError>
    where
        E: Fn(&str) -> Result<Option<String>, AuthError>,
    {
        Self::with_credential_plan(transport, mistral_credential_plan(&environment)?)
    }
}

impl<T, H> MistralOcrConfig<T, H> {
    pub fn with_hooks(transport: T, hooks: H) -> Result<Self, AuthError> {
        Ok(Self::with_hooks_and_credential_plan(
            transport,
            hooks,
            mistral_credential_plan(&environment_lookup)?,
        ))
    }

    fn with_credential_plan(transport: T, plan: CredentialPlan) -> Result<Self, AuthError>
    where
        H: Default,
    {
        Ok(Self::with_hooks_and_credential_plan(
            transport,
            H::default(),
            plan,
        ))
    }

    fn with_hooks_and_credential_plan(transport: T, hooks: H, plan: CredentialPlan) -> Self {
        Self {
            pipeline: Pipeline::new(
                ApiKeyAuth::bearer("Mistral", plan)
                    .existing_credential_policy(ExistingCredentialPolicy::Accept),
                MistralOcrEndpoint,
                MistralOcrAdapter,
                JsonExecution::new(transport),
                hooks,
            ),
        }
    }
}

fn mistral_credential_plan(
    environment: &dyn Fn(&str) -> Result<Option<String>, AuthError>,
) -> Result<CredentialPlan, AuthError> {
    Ok(CredentialPlan::fallback([
        CredentialPlan::Reference(CredentialRef::Request("api_key".into())),
        CredentialLocation::Environment("MISTRAL_API_KEY".into())
            .compile(environment, &|_| Ok(None))?,
    ]))
}

fn environment_lookup(name: &str) -> Result<Option<String>, AuthError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(AuthError::Configuration(
            AuthConfigurationError::CredentialLoad,
        )),
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

#[cfg(test)]
mod tests {
    use litellm_auth::{CredentialSource, DynamicCredentials};

    use super::mistral_credential_plan;

    #[tokio::test]
    async fn compiled_environment_plan_retains_provider_source() {
        let plan = mistral_credential_plan(&|name| {
            Ok((name == "MISTRAL_API_KEY").then(|| "environment-key".into()))
        })
        .unwrap();

        let resolution = plan.resolve(&DynamicCredentials::default()).await.unwrap();

        assert_eq!(resolution.source(), Some(CredentialSource::Environment));
    }
}
