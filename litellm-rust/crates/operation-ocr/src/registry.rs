use litellm_operation::{ApiKeyAuth, Error, JsonExecution, NoHooks, Pipeline};
use litellm_transport::Transport;

use crate::endpoints::{
    AzureDocumentIntelligenceEndpoint, AzureMistralEndpoint, MistralEndpoint, ReductoEndpoint,
    ReductoLegacyEndpoint,
};
use crate::execution::{DocumentIntelligenceExecution, InlineJsonExecution, ReductoExecution};
use crate::plan::{
    DocumentIntelligenceTransformation, MistralTransformation, ReductoLegacyTransformation,
    ReductoV3Transformation,
};
use crate::types::{OcrCall, OcrCallContext, OcrResponse};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OcrComposition {
    Mistral,
    AzureMistral,
    AzureDocumentIntelligence,
    ReductoLegacy,
    ReductoV3,
}

pub fn resolve_composition(
    model: &str,
    custom_llm_provider: Option<&str>,
) -> Result<(String, OcrComposition), Error> {
    let (provider, model) = match custom_llm_provider {
        Some(provider) => (provider.to_string(), model.to_string()),
        None => match model.split_once('/') {
            Some((provider, model)) if !provider.is_empty() && !model.is_empty() => {
                (provider.to_string(), model.to_string())
            }
            _ => ("mistral".to_string(), model.to_string()),
        },
    };
    let composition = match provider.as_str() {
        "mistral" => OcrComposition::Mistral,
        "azure_ai" => {
            if is_document_intelligence_model(&model) {
                OcrComposition::AzureDocumentIntelligence
            } else {
                OcrComposition::AzureMistral
            }
        }
        "reducto" if model.eq_ignore_ascii_case("parse-legacy") => OcrComposition::ReductoLegacy,
        "reducto" => OcrComposition::ReductoV3,
        other => {
            return Err(Error::InvalidRequest(format!(
                "unsupported ocr provider: {other}"
            )));
        }
    };
    Ok((model, composition))
}

fn is_document_intelligence_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("doc-intelligence") || model.contains("documentintelligence")
}

const AZURE_DI_SUBSCRIPTION_HEADER: &str = "ocp-apim-subscription-key";

pub async fn dispatch<T>(
    composition: OcrComposition,
    transport: T,
    call: OcrCall,
    context: &OcrCallContext,
) -> Result<OcrResponse, Error>
where
    T: Transport + Send + Sync,
{
    match composition {
        OcrComposition::Mistral => {
            Pipeline::new(
                ApiKeyAuth::bearer("mistral"),
                MistralEndpoint,
                MistralTransformation,
                JsonExecution::new(transport),
                NoHooks,
            )
            .handle(call, context)
            .await
        }
        OcrComposition::AzureMistral => {
            Pipeline::new(
                ApiKeyAuth::bearer("azure"),
                AzureMistralEndpoint,
                MistralTransformation,
                InlineJsonExecution::new(transport),
                NoHooks,
            )
            .handle(call, context)
            .await
        }
        OcrComposition::AzureDocumentIntelligence => {
            Pipeline::new(
                ApiKeyAuth::header("azure", AZURE_DI_SUBSCRIPTION_HEADER),
                AzureDocumentIntelligenceEndpoint,
                DocumentIntelligenceTransformation,
                DocumentIntelligenceExecution::new(transport),
                NoHooks,
            )
            .handle(call, context)
            .await
        }
        OcrComposition::ReductoLegacy => {
            Pipeline::new(
                ApiKeyAuth::bearer("reducto"),
                ReductoLegacyEndpoint,
                ReductoLegacyTransformation,
                ReductoExecution::new(transport),
                NoHooks,
            )
            .handle(call, context)
            .await
        }
        OcrComposition::ReductoV3 => {
            Pipeline::new(
                ApiKeyAuth::bearer("reducto"),
                ReductoEndpoint,
                ReductoV3Transformation,
                ReductoExecution::new(transport),
                NoHooks,
            )
            .handle(call, context)
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_prefix_and_explicit_provider_select_compositions() {
        assert_eq!(
            resolve_composition("mistral/ocr-latest", None).unwrap().1,
            OcrComposition::Mistral
        );
        assert_eq!(
            resolve_composition("prebuilt-read", Some("azure_ai"))
                .unwrap()
                .1,
            OcrComposition::AzureMistral
        );
        assert_eq!(
            resolve_composition("prebuilt-doc-intelligence", Some("azure_ai"))
                .unwrap()
                .1,
            OcrComposition::AzureDocumentIntelligence
        );
        assert_eq!(
            resolve_composition("parse-legacy", Some("reducto"))
                .unwrap()
                .1,
            OcrComposition::ReductoLegacy
        );
        assert_eq!(
            resolve_composition("reducto/parse", None).unwrap().1,
            OcrComposition::ReductoV3
        );
        assert!(resolve_composition("model", None).unwrap().1 == OcrComposition::Mistral);
        assert!(resolve_composition("model", Some("unknown")).is_err());
    }
}
