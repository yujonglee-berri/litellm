use crate::Error;
use crate::operation::{OperationPlan, Provider, TransformationKind, WireOperation};
use crate::routing_utils::provider::{CustomLlmProvider, get_custom_llm_provider};

use super::types::OcrOperation;

macro_rules! define_pipeline_types {
    ($( $variant:ident, $auth:expr, $endpoint:expr, $transformation:expr, $execution:expr, $provider:ident; )+) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum OcrPlan {
            $( $variant, )+
        }
    };
}

macro_rules! for_each_ocr_pipeline {
    ($callback:ident) => {
        $callback! {
            Mistral, $crate::ocr::auth::MistralAuth, $crate::ocr::endpoints::MistralEndpoint, $crate::ocr::transformations::mistral::MistralOcrTransformation, $crate::ocr::execution::JsonExecution, Mistral;
            AzureMistral, $crate::ocr::auth::AzureOcrAuth::Mistral, $crate::ocr::endpoints::AzureMistralEndpoint, $crate::ocr::transformations::mistral::MistralOcrTransformation, $crate::ocr::execution::InlineJsonExecution, AzureAi;
            AzureDocumentIntelligence, $crate::ocr::auth::AzureOcrAuth::DocumentIntelligence, $crate::ocr::endpoints::AzureDocumentIntelligenceEndpoint, $crate::ocr::transformations::document_intelligence::DocumentIntelligenceTransformation, $crate::ocr::execution::DocumentIntelligenceExecution, AzureAi;
            ReductoLegacy, $crate::ocr::auth::ReductoAuth, $crate::ocr::endpoints::ReductoEndpoint, $crate::ocr::transformations::reducto::ReductoLegacyTransformation, $crate::ocr::execution::ReductoExecution, Reducto;
            ReductoV3, $crate::ocr::auth::ReductoAuth, $crate::ocr::endpoints::ReductoEndpoint, $crate::ocr::transformations::reducto::ReductoV3Transformation, $crate::ocr::execution::ReductoExecution, Reducto;
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

impl Provider for OcrProvider {
    fn name(self) -> &'static str {
        match self {
            Self::Mistral => "mistral",
            Self::AzureAi => "azure_ai",
            Self::Reducto => "reducto",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OcrWireOperation {
    MistralOcr,
    AzureDocumentIntelligenceAnalyze,
    ReductoParseLegacy,
    ReductoParse,
}

impl WireOperation for OcrWireOperation {
    fn name(self) -> &'static str {
        match self {
            Self::MistralOcr => "mistral.ocr",
            Self::AzureDocumentIntelligenceAnalyze => "azure.document_intelligence.analyze",
            Self::ReductoParseLegacy => "reducto.parse_legacy",
            Self::ReductoParse => "reducto.parse",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OcrTransformationKind {
    Mistral,
    DocumentIntelligence,
    ReductoLegacy,
    ReductoV3,
}

impl TransformationKind for OcrTransformationKind {
    type WireOperation = OcrWireOperation;

    fn wire_operation(self) -> Self::WireOperation {
        match self {
            Self::Mistral => OcrWireOperation::MistralOcr,
            Self::DocumentIntelligence => OcrWireOperation::AzureDocumentIntelligenceAnalyze,
            Self::ReductoLegacy => OcrWireOperation::ReductoParseLegacy,
            Self::ReductoV3 => OcrWireOperation::ReductoParse,
        }
    }
}

impl OperationPlan for OcrPlan {
    type Operation = OcrOperation;
    type Provider = OcrProvider;
    type WireOperation = OcrWireOperation;
    type Transformation = OcrTransformationKind;

    fn provider(self) -> Self::Provider {
        match self {
            Self::Mistral => OcrProvider::Mistral,
            Self::AzureMistral | Self::AzureDocumentIntelligence => OcrProvider::AzureAi,
            Self::ReductoLegacy | Self::ReductoV3 => OcrProvider::Reducto,
        }
    }

    fn transformation(self) -> Self::Transformation {
        match self {
            Self::Mistral | Self::AzureMistral => OcrTransformationKind::Mistral,
            Self::AzureDocumentIntelligence => OcrTransformationKind::DocumentIntelligence,
            Self::ReductoLegacy => OcrTransformationKind::ReductoLegacy,
            Self::ReductoV3 => OcrTransformationKind::ReductoV3,
        }
    }
}

pub(crate) fn resolve_plan(
    model: &str,
    custom_llm_provider: Option<&str>,
) -> Result<(String, OcrPlan), Error> {
    let provider =
        get_custom_llm_provider(model, custom_llm_provider).unwrap_or(CustomLlmProvider {
            model,
            custom_llm_provider: OcrProvider::Mistral.name(),
        });
    let typed_provider = match provider.custom_llm_provider {
        "mistral" => OcrProvider::Mistral,
        "azure_ai" => OcrProvider::AzureAi,
        "reducto" => OcrProvider::Reducto,
        value => return Err(Error::InvalidProvider(value.to_string())),
    };
    match typed_provider {
        OcrProvider::Mistral => Ok((provider.model.to_string(), OcrPlan::Mistral)),
        OcrProvider::AzureAi if is_document_intelligence_model(provider.model) => Ok((
            provider.model.to_string(),
            OcrPlan::AzureDocumentIntelligence,
        )),
        OcrProvider::AzureAi => Ok((provider.model.to_string(), OcrPlan::AzureMistral)),
        OcrProvider::Reducto if provider.model.eq_ignore_ascii_case("parse-legacy") => {
            Ok((provider.model.to_string(), OcrPlan::ReductoLegacy))
        }
        OcrProvider::Reducto => Ok((provider.model.to_string(), OcrPlan::ReductoV3)),
    }
}

fn is_document_intelligence_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("doc-intelligence") || model.contains("documentintelligence")
}

#[cfg(test)]
mod tests {
    use crate::operation::{
        CapabilitySupport, DeliveryMode, Fidelity, OperationPlan, OperationTransformation,
        Provider, WireOperation,
    };

    use super::*;
    use crate::ocr::transformations::document_intelligence::DocumentIntelligenceTransformation;
    use crate::ocr::transformations::mistral::MistralOcrTransformation;

    #[test]
    fn azure_can_bind_to_two_independent_wire_operations() {
        let mistral = OcrPlan::AzureMistral;
        let document_intelligence = OcrPlan::AzureDocumentIntelligence;

        assert_eq!(mistral.provider().name(), "azure_ai");
        assert_eq!(document_intelligence.provider().name(), "azure_ai");
        assert_eq!(mistral.wire_operation().name(), "mistral.ocr");
        assert_eq!(
            document_intelligence.wire_operation().name(),
            "azure.document_intelligence.analyze"
        );
        assert_eq!(mistral.transformation(), OcrTransformationKind::Mistral);
        assert_eq!(
            document_intelligence.transformation(),
            OcrTransformationKind::DocumentIntelligence
        );
        assert_eq!(
            MistralOcrTransformation.wire_operation(),
            mistral.wire_operation()
        );
        assert_eq!(MistralOcrTransformation.fidelity(), Fidelity::Exact);
        assert_eq!(
            mistral.delivery_support(DeliveryMode::Complete),
            CapabilitySupport::Supported
        );
        assert_eq!(
            DocumentIntelligenceTransformation.wire_operation(),
            document_intelligence.wire_operation()
        );
    }

    #[test]
    fn mistral_wire_format_can_be_hosted_by_two_providers() {
        let mistral = OcrPlan::Mistral;
        let azure = OcrPlan::AzureMistral;

        assert_ne!(mistral.provider(), azure.provider());
        assert_eq!(mistral.wire_operation(), azure.wire_operation());
        assert_eq!(mistral.transformation(), azure.transformation());
    }
}
