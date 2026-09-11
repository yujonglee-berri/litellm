use litellm_auth::{ExistingHeaderBehavior, HeaderAuth, ResolveAuth, ResolvedAuth, SecretValue};
use litellm_operation::Error;

use crate::types::{OcrCall, OcrCallContext};

fn bearer_authenticator(key: &SecretValue, provider: &'static str) -> Result<HeaderAuth, Error> {
    HeaderAuth::bearer(key, ExistingHeaderBehavior::Preserve)
        .map_err(|_| Error::MissingApiKey(provider))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MistralAuth;

impl ResolveAuth<OcrCall, OcrCallContext> for MistralAuth {
    type Authenticator = HeaderAuth;
    type Context = ();
    type Error = Error;

    async fn resolve(
        &self,
        _call: &OcrCall,
        context: &OcrCallContext,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, Error> {
        let key = context
            .api_key
            .as_ref()
            .ok_or(Error::MissingApiKey("mistral"))?;
        Ok(ResolvedAuth {
            authenticator: bearer_authenticator(key, "mistral")?,
            headers: Vec::new(),
            context: (),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReductoAuth;

impl ResolveAuth<OcrCall, OcrCallContext> for ReductoAuth {
    type Authenticator = HeaderAuth;
    type Context = ();
    type Error = Error;

    async fn resolve(
        &self,
        _call: &OcrCall,
        context: &OcrCallContext,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, Error> {
        let key = context
            .api_key
            .as_ref()
            .ok_or(Error::MissingApiKey("reducto"))?;
        Ok(ResolvedAuth {
            authenticator: bearer_authenticator(key, "reducto")?,
            headers: Vec::new(),
            context: (),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AzureOcrAuth {
    Mistral,
    DocumentIntelligence,
}

impl ResolveAuth<OcrCall, OcrCallContext> for AzureOcrAuth {
    type Authenticator = HeaderAuth;
    type Context = ();
    type Error = Error;

    async fn resolve(
        &self,
        _call: &OcrCall,
        context: &OcrCallContext,
    ) -> Result<ResolvedAuth<HeaderAuth, ()>, Error> {
        let key = context
            .api_key
            .as_ref()
            .ok_or(Error::MissingApiKey("azure"))?;
        match self {
            Self::Mistral => Ok(ResolvedAuth {
                authenticator: bearer_authenticator(key, "azure")?,
                headers: Vec::new(),
                context: (),
            }),
            Self::DocumentIntelligence => Ok(ResolvedAuth {
                authenticator: HeaderAuth::new(
                    [(
                        reqwest::header::HeaderName::from_static("ocp-apim-subscription-key"),
                        key.clone(),
                    )],
                    ExistingHeaderBehavior::Preserve,
                )?,
                headers: Vec::new(),
                context: (),
            }),
        }
    }
}
