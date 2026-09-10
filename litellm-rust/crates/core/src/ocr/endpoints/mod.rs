mod azure;
mod mistral;
mod reducto;

use super::error::OcrError;
use super::types::LiteLLMOcrRequest;

pub(crate) use azure::{AzureDocumentIntelligenceEndpoint, AzureMistralEndpoint};
pub(crate) use mistral::MistralEndpoint;
pub(crate) use reducto::{ReductoEndpoint, complete_url as complete_reducto_url};

pub(crate) trait ResolveOcrEndpoint<AuthContext, Params>: Send + Sync {
    fn resolve(
        &self,
        request: &LiteLLMOcrRequest,
        auth: &AuthContext,
        params: &Params,
    ) -> Result<String, OcrError>;
}
