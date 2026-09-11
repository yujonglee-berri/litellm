use litellm_operation::{
    Complete, Fidelity, OperationPlan, OperationTransformation, Provider, SupportsWire,
    TransformationForDelivery, TransformationKind, WireOperation,
};

use crate::Ocr;

macro_rules! marker {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub struct $name;
    };
}

marker!(Mistral);
marker!(AzureAi);
marker!(Reducto);
marker!(MistralOcr);
marker!(AzureDocumentIntelligenceAnalyze);
marker!(ReductoParseLegacy);
marker!(ReductoParse);
marker!(MistralTransformation);
marker!(DocumentIntelligenceTransformation);
marker!(ReductoLegacyTransformation);
marker!(ReductoV3Transformation);

impl Provider for Mistral {
    const NAME: &'static str = "mistral";
}

impl Provider for AzureAi {
    const NAME: &'static str = "azure_ai";
}

impl Provider for Reducto {
    const NAME: &'static str = "reducto";
}

impl WireOperation for MistralOcr {
    const NAME: &'static str = "mistral.ocr";
}

impl WireOperation for AzureDocumentIntelligenceAnalyze {
    const NAME: &'static str = "azure.document_intelligence.analyze";
}

impl WireOperation for ReductoParseLegacy {
    const NAME: &'static str = "reducto.parse_legacy";
}

impl WireOperation for ReductoParse {
    const NAME: &'static str = "reducto.parse";
}

impl SupportsWire<MistralOcr, Complete> for Mistral {}
impl SupportsWire<MistralOcr, Complete> for AzureAi {}
impl SupportsWire<AzureDocumentIntelligenceAnalyze, Complete> for AzureAi {}
impl SupportsWire<ReductoParseLegacy, Complete> for Reducto {}
impl SupportsWire<ReductoParse, Complete> for Reducto {}

impl OperationTransformation<Ocr, MistralOcr> for MistralTransformation {
    const FIDELITY: Fidelity = Fidelity::Exact;
}

impl TransformationKind<Ocr, MistralOcr> for MistralTransformation {
    const NAME: &'static str = "ocr.to_mistral";
}

impl TransformationForDelivery<Ocr, MistralOcr, Complete> for MistralTransformation {}

impl OperationTransformation<Ocr, AzureDocumentIntelligenceAnalyze>
    for DocumentIntelligenceTransformation
{
    const FIDELITY: Fidelity = Fidelity::Exact;
}

impl TransformationKind<Ocr, AzureDocumentIntelligenceAnalyze>
    for DocumentIntelligenceTransformation
{
    const NAME: &'static str = "ocr.to_azure_document_intelligence";
}

impl TransformationForDelivery<Ocr, AzureDocumentIntelligenceAnalyze, Complete>
    for DocumentIntelligenceTransformation
{
}

impl OperationTransformation<Ocr, ReductoParseLegacy> for ReductoLegacyTransformation {
    const FIDELITY: Fidelity = Fidelity::Lossy;
}

impl TransformationKind<Ocr, ReductoParseLegacy> for ReductoLegacyTransformation {
    const NAME: &'static str = "ocr.to_reducto_legacy";
}

impl TransformationForDelivery<Ocr, ReductoParseLegacy, Complete> for ReductoLegacyTransformation {}

impl OperationTransformation<Ocr, ReductoParse> for ReductoV3Transformation {
    const FIDELITY: Fidelity = Fidelity::Exact;
}

impl TransformationKind<Ocr, ReductoParse> for ReductoV3Transformation {
    const NAME: &'static str = "ocr.to_reducto_v3";
}

impl TransformationForDelivery<Ocr, ReductoParse, Complete> for ReductoV3Transformation {}

pub type MistralOcrPlan = OperationPlan<Ocr, Mistral, MistralOcr, Complete, MistralTransformation>;
pub type AzureMistralPlan =
    OperationPlan<Ocr, AzureAi, MistralOcr, Complete, MistralTransformation>;
pub type AzureDocumentIntelligencePlan = OperationPlan<
    Ocr,
    AzureAi,
    AzureDocumentIntelligenceAnalyze,
    Complete,
    DocumentIntelligenceTransformation,
>;
pub type ReductoLegacyPlan =
    OperationPlan<Ocr, Reducto, ReductoParseLegacy, Complete, ReductoLegacyTransformation>;
pub type ReductoV3Plan =
    OperationPlan<Ocr, Reducto, ReductoParse, Complete, ReductoV3Transformation>;

pub const fn mistral() -> MistralOcrPlan {
    OperationPlan::new(Mistral, MistralOcr, MistralTransformation)
}

pub const fn azure_mistral() -> AzureMistralPlan {
    OperationPlan::new(AzureAi, MistralOcr, MistralTransformation)
}

pub const fn azure_document_intelligence() -> AzureDocumentIntelligencePlan {
    OperationPlan::new(
        AzureAi,
        AzureDocumentIntelligenceAnalyze,
        DocumentIntelligenceTransformation,
    )
}

pub const fn reducto_legacy() -> ReductoLegacyPlan {
    OperationPlan::new(Reducto, ReductoParseLegacy, ReductoLegacyTransformation)
}

pub const fn reducto_v3() -> ReductoV3Plan {
    OperationPlan::new(Reducto, ReductoParse, ReductoV3Transformation)
}

#[cfg(test)]
mod tests {
    use litellm_operation::{Delivery, OperationTransformation, Provider, WireOperation};

    use super::*;

    #[test]
    fn azure_can_host_two_independent_wire_operations() {
        let mistral = azure_mistral();
        let document_intelligence = azure_document_intelligence();

        fn accepts_complete(_: AzureMistralPlan) {}

        accepts_complete(mistral);

        assert_eq!(AzureAi::NAME, "azure_ai");
        assert_eq!(mistral.delivery(), Delivery::Complete);
        assert_eq!(mistral.wire_operation(), &MistralOcr);
        assert_eq!(
            document_intelligence.wire_operation(),
            &AzureDocumentIntelligenceAnalyze
        );
    }

    #[test]
    fn mistral_wire_shape_can_be_selected_by_two_providers() {
        assert_eq!(mistral().wire_operation(), azure_mistral().wire_operation());
        assert_eq!(MistralOcr::NAME, "mistral.ocr");
        assert_eq!(MistralTransformation::FIDELITY, Fidelity::Exact);
    }

    #[test]
    fn reducto_versions_are_explicit_plans() {
        assert_eq!(reducto_legacy().wire_operation(), &ReductoParseLegacy);
        assert_eq!(reducto_v3().wire_operation(), &ReductoParse);
    }
}
