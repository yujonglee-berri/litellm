use crate::Error;
use crate::routing_utils::provider::{CustomLlmProvider, get_custom_llm_provider};

macro_rules! define_pipeline_types {
    ($( $variant:ident, $auth:expr, $endpoint:expr, $codec:expr, $execution:expr, $provider:ident; )+) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum OcrPipelineKind {
            $( $variant, )+
        }

        impl OcrPipelineKind {
            pub(crate) const fn provider(self) -> OcrProvider {
                match self {
                    $( Self::$variant => OcrProvider::$provider, )+
                }
            }
        }
    };
}

macro_rules! for_each_ocr_pipeline {
    ($callback:ident) => {
        $callback! {
            Mistral, $crate::ocr::auth::MistralAuth, $crate::ocr::endpoints::MistralEndpoint, $crate::ocr::codecs::mistral::MistralOcrCodec, $crate::ocr::execution::JsonExecution, Mistral;
            AzureMistral, $crate::ocr::auth::AzureOcrAuth::Mistral, $crate::ocr::endpoints::AzureMistralEndpoint, $crate::ocr::codecs::mistral::MistralOcrCodec, $crate::ocr::execution::InlineJsonExecution, AzureAi;
            AzureDocumentIntelligence, $crate::ocr::auth::AzureOcrAuth::DocumentIntelligence, $crate::ocr::endpoints::AzureDocumentIntelligenceEndpoint, $crate::ocr::codecs::document_intelligence::DocumentIntelligenceCodec, $crate::ocr::execution::DocumentIntelligenceExecution, AzureAi;
            ReductoLegacy, $crate::ocr::auth::ReductoAuth, $crate::ocr::endpoints::ReductoEndpoint, $crate::ocr::codecs::reducto::ReductoLegacyCodec, $crate::ocr::execution::ReductoExecution, Reducto;
            ReductoV3, $crate::ocr::auth::ReductoAuth, $crate::ocr::endpoints::ReductoEndpoint, $crate::ocr::codecs::reducto::ReductoV3Codec, $crate::ocr::execution::ReductoExecution, Reducto;
        }
    };
}

for_each_ocr_pipeline!(define_pipeline_types);

pub(crate) use for_each_ocr_pipeline;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OcrProvider {
    Mistral,
    AzureAi,
    Reducto,
}

impl OcrProvider {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Mistral => "mistral",
            Self::AzureAi => "azure_ai",
            Self::Reducto => "reducto",
        }
    }
}

pub(crate) fn resolve_wire_pipeline(
    model: &str,
    custom_llm_provider: Option<&str>,
) -> Result<(String, OcrPipelineKind), Error> {
    let provider =
        get_custom_llm_provider(model, custom_llm_provider).unwrap_or(CustomLlmProvider {
            model,
            custom_llm_provider: OcrProvider::Mistral.as_str(),
        });
    let typed_provider = match provider.custom_llm_provider {
        "mistral" => OcrProvider::Mistral,
        "azure_ai" => OcrProvider::AzureAi,
        "reducto" => OcrProvider::Reducto,
        value => return Err(Error::InvalidProvider(value.to_string())),
    };
    match typed_provider {
        OcrProvider::Mistral => Ok((provider.model.to_string(), OcrPipelineKind::Mistral)),
        OcrProvider::AzureAi if is_document_intelligence_model(provider.model) => Ok((
            provider.model.to_string(),
            OcrPipelineKind::AzureDocumentIntelligence,
        )),
        OcrProvider::AzureAi => Ok((provider.model.to_string(), OcrPipelineKind::AzureMistral)),
        OcrProvider::Reducto if provider.model.eq_ignore_ascii_case("parse-legacy") => {
            Ok((provider.model.to_string(), OcrPipelineKind::ReductoLegacy))
        }
        OcrProvider::Reducto => Ok((provider.model.to_string(), OcrPipelineKind::ReductoV3)),
    }
}

fn is_document_intelligence_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("doc-intelligence") || model.contains("documentintelligence")
}
