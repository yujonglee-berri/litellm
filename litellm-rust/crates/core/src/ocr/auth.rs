use std::sync::OnceLock;

use reqwest::header::HeaderName;

use crate::auth::azure::{AzureAuthInputs, AzureAuthService};
use crate::auth::error::AuthConfigurationError;
use crate::auth::{
    ExistingHeaderBehavior, HeaderAuth, InputSource, ResolveAuth, ResolvedAuth, SecretValue,
    Sourced,
};
use crate::constants::AZURE_DI_SUBSCRIPTION_HEADER;
use crate::ocr::OcrClient;
use crate::ocr::error::OcrError;
use crate::ocr::prepare::credential_env;
use crate::ocr::types::{LiteLLMOcrRequest, OcrConnection};
use crate::{AuthError, Error};

const AZURE_AI_API_KEY_ENV: &str = "AZURE_AI_API_KEY";
const AZURE_DI_API_KEY_ENV: &str = "AZURE_DOCUMENT_INTELLIGENCE_API_KEY";
const MISTRAL_API_KEY_ENV: &str = "MISTRAL_API_KEY";
const REDUCTO_API_KEY_ENV: &str = "REDUCTO_API_KEY";

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MistralAuth;

impl ResolveAuth<LiteLLMOcrRequest, OcrClient> for MistralAuth {
    type Authenticator = HeaderAuth;
    type Context = ();
    type Error = OcrError;

    async fn resolve(
        &self,
        request: &LiteLLMOcrRequest,
        _services: &OcrClient,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, OcrError> {
        resolve_bearer(
            &request.connection,
            MISTRAL_API_KEY_ENV,
            Error::MissingApiKey {
                provider: "Mistral",
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReductoAuth;

impl ResolveAuth<LiteLLMOcrRequest, OcrClient> for ReductoAuth {
    type Authenticator = HeaderAuth;
    type Context = ();
    type Error = OcrError;

    async fn resolve(
        &self,
        request: &LiteLLMOcrRequest,
        _services: &OcrClient,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, OcrError> {
        resolve_bearer(
            &request.connection,
            REDUCTO_API_KEY_ENV,
            Error::MissingReductoApiKey,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AzureOcrAuth {
    Mistral,
    DocumentIntelligence,
}

impl ResolveAuth<LiteLLMOcrRequest, OcrClient> for AzureOcrAuth {
    type Authenticator = HeaderAuth;
    type Context = ();
    type Error = OcrError;

    #[tracing::instrument(
        name = "validate_environment",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    async fn resolve(
        &self,
        request: &LiteLLMOcrRequest,
        _services: &OcrClient,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, OcrError> {
        let config = AzureAuthInputs::from_sourced_optional_params(
            &request.optional_params,
            &request.input_sources,
        )
        .map_err(Error::from)?;
        self.resolve_auth(&request.connection, &config).await
    }
}

impl AzureOcrAuth {
    async fn resolve_auth(
        self,
        connection: &OcrConnection,
        config: &AzureAuthInputs,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, OcrError> {
        if self.has_existing_credential(&connection.extra_headers) {
            validate_destination(connection, connection.extra_headers_source)?;
            return Ok(resolved_auth(connection, empty_auth()?));
        }
        let key = nonblank(connection.api_key.clone())
            .map(|value| Sourced::new(value, connection.api_key_source))
            .or_else(|| {
                nonblank(credential_env(self.api_key_env()))
                    .map(|value| Sourced::new(value, InputSource::Environment))
            });
        if let Some(key) = key {
            validate_destination(connection, key.source())?;
            return Ok(resolved_auth(connection, self.key_auth(key.into_value())?));
        }
        let token = resolve_entra(config)
            .await?
            .ok_or_else(|| self.missing_credentials())?;
        validate_destination(connection, token.source())?;
        Ok(resolved_auth(connection, bearer_auth(token.into_value())?))
    }

    fn api_key_env(self) -> &'static str {
        match self {
            Self::Mistral => AZURE_AI_API_KEY_ENV,
            Self::DocumentIntelligence => AZURE_DI_API_KEY_ENV,
        }
    }

    fn has_existing_credential(self, headers: &[(String, String)]) -> bool {
        crate::http_utils::has_header(headers, "authorization")
            || matches!(self, Self::DocumentIntelligence)
                && crate::http_utils::has_header(headers, AZURE_DI_SUBSCRIPTION_HEADER)
    }

    fn key_auth(self, key: String) -> Result<HeaderAuth, OcrError> {
        match self {
            Self::Mistral => bearer_auth(key),
            Self::DocumentIntelligence => named_header_auth(AZURE_DI_SUBSCRIPTION_HEADER, key),
        }
    }

    fn missing_credentials(self) -> OcrError {
        match self {
            Self::Mistral => Error::MissingAzureAiCredentials.into(),
            Self::DocumentIntelligence => Error::MissingAzureDocumentIntelligenceCredentials.into(),
        }
    }
}

#[tracing::instrument(
    name = "validate_environment",
    target = "litellm::function_trace",
    level = "trace",
    skip_all
)]
fn resolve_bearer(
    connection: &OcrConnection,
    env_name: &str,
    missing: Error,
) -> Result<ResolvedAuth<HeaderAuth, ()>, OcrError> {
    if crate::http_utils::has_header(&connection.extra_headers, "authorization") {
        return Ok(resolved_auth(connection, empty_auth()?));
    }
    let key = connection
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .or_else(|| {
            credential_env(env_name)
                .map(|key| key.trim().to_string())
                .filter(|key| !key.is_empty())
        })
        .ok_or(missing)?;
    Ok(resolved_auth(connection, bearer_auth(key)?))
}

fn resolved_auth(
    connection: &OcrConnection,
    authenticator: HeaderAuth,
) -> ResolvedAuth<HeaderAuth, ()> {
    ResolvedAuth {
        authenticator,
        headers: connection.extra_headers.clone(),
        context: (),
    }
}

fn empty_auth() -> Result<HeaderAuth, OcrError> {
    HeaderAuth::new([], ExistingHeaderBehavior::Preserve).map_err(auth_error)
}

fn bearer_auth(key: String) -> Result<HeaderAuth, OcrError> {
    HeaderAuth::bearer(&SecretValue::new(key), ExistingHeaderBehavior::Preserve).map_err(auth_error)
}

fn named_header_auth(name: &'static str, key: String) -> Result<HeaderAuth, OcrError> {
    let name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| auth_error(AuthError::InvalidHeader))?;
    HeaderAuth::new(
        [(name, SecretValue::new(key))],
        ExistingHeaderBehavior::Preserve,
    )
    .map_err(auth_error)
}

fn auth_error(error: AuthError) -> OcrError {
    Error::from(error).into()
}

async fn resolve_entra(config: &AzureAuthInputs) -> Result<Option<Sourced<String>>, Error> {
    static SERVICE: OnceLock<AzureAuthService> = OnceLock::new();
    SERVICE
        .get_or_init(AzureAuthService::default)
        .get_azure_ad_token(config, &credential_env)
        .await
        .map(|credential| {
            credential.map(|credential| {
                let source = credential.source();
                let value = credential.value().secret().expose().to_string();
                Sourced::new(value, source)
            })
        })
        .map_err(Error::from)
}

fn validate_destination(
    connection: &OcrConnection,
    credential_source: InputSource,
) -> Result<(), OcrError> {
    if connection.api_base.is_some()
        && connection.api_base_source == InputSource::Request
        && credential_source != InputSource::Request
    {
        return Err(Error::from(AuthError::Configuration(
            AuthConfigurationError::RequestAzureCredentialDestination,
        ))
        .into());
    }
    Ok(())
}

fn nonblank(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
