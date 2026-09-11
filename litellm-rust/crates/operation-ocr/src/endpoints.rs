use litellm_operation::{Error, ResolveEndpoint};

use crate::codec::{DocumentIntelligenceParams, MistralParams};
use crate::types::{OcrCall, OcrCallContext};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MistralEndpoint;

impl<AuthContext> ResolveEndpoint<AuthContext, MistralParams, OcrCall, OcrCallContext>
    for MistralEndpoint
{
    fn resolve(
        &self,
        _call: &OcrCall,
        context: &OcrCallContext,
        _auth: &AuthContext,
        _params: &MistralParams,
    ) -> Result<String, Error> {
        let base = context
            .api_base
            .as_deref()
            .unwrap_or("https://api.mistral.ai/v1");
        Ok(format!("{base}/ocr"))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AzureMistralEndpoint;

impl<AuthContext> ResolveEndpoint<AuthContext, MistralParams, OcrCall, OcrCallContext>
    for AzureMistralEndpoint
{
    fn resolve(
        &self,
        _call: &OcrCall,
        context: &OcrCallContext,
        _auth: &AuthContext,
        _params: &MistralParams,
    ) -> Result<String, Error> {
        let base = context
            .api_base
            .as_deref()
            .ok_or_else(|| Error::InvalidRequest("azure_ai ocr requires an api_base".into()))?;
        Ok(format!("{base}/anthropic/v1/ocr"))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AzureDocumentIntelligenceEndpoint;

impl<AuthContext> ResolveEndpoint<AuthContext, DocumentIntelligenceParams, OcrCall, OcrCallContext>
    for AzureDocumentIntelligenceEndpoint
{
    fn resolve(
        &self,
        call: &OcrCall,
        context: &OcrCallContext,
        _auth: &AuthContext,
        params: &DocumentIntelligenceParams,
    ) -> Result<String, Error> {
        let base = context
            .api_base
            .as_deref()
            .ok_or_else(|| Error::InvalidRequest("azure_ai ocr requires an api_base".into()))?;
        let mut query: Vec<String> = Vec::new();
        if let Some(pages) = &params.pages {
            query.push(format!("pages={pages}"));
        }
        if !params.features.is_empty() {
            query.push(format!("features={}", params.features.join(",")));
        }
        let query = if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query.join("&"))
        };
        Ok(format!(
            "{base}/documentintelligence/documentModels/{}:analyze{query}",
            call.model
        ))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReductoEndpoint;

impl<AuthContext> ResolveEndpoint<AuthContext, (), OcrCall, OcrCallContext> for ReductoEndpoint {
    fn resolve(
        &self,
        _call: &OcrCall,
        context: &OcrCallContext,
        _auth: &AuthContext,
        _params: &(),
    ) -> Result<String, Error> {
        let base = context
            .api_base
            .as_deref()
            .unwrap_or("https://api.reducto.ai/v3");
        Ok(format!("{base}/parse"))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReductoLegacyEndpoint;

impl<AuthContext> ResolveEndpoint<AuthContext, (), OcrCall, OcrCallContext>
    for ReductoLegacyEndpoint
{
    fn resolve(
        &self,
        _call: &OcrCall,
        context: &OcrCallContext,
        _auth: &AuthContext,
        _params: &(),
    ) -> Result<String, Error> {
        let base = context
            .api_base
            .as_deref()
            .unwrap_or("https://api.reducto.ai");
        Ok(format!("{base}/v1/parse"))
    }
}
