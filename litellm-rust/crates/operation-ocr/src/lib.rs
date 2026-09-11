mod auth;
mod codec;
mod endpoints;
mod execution;
mod plan;
mod registry;
mod types;

pub use auth::{AzureOcrAuth, MistralAuth, ReductoAuth};
pub use codec::{
    DocumentIntelligenceParams, DocumentIntelligenceRequest, DocumentIntelligenceResponse,
    MistralOcrPage, MistralOcrRequest, MistralOcrResponse, MistralParams, ReductoParseRequest,
    ReductoParseResponse,
};
pub use endpoints::{
    AzureDocumentIntelligenceEndpoint, AzureMistralEndpoint, MistralEndpoint, ReductoEndpoint,
    ReductoLegacyEndpoint,
};
pub use execution::{DocumentIntelligenceExecution, InlineJsonExecution, ReductoExecution};
pub use litellm_operation::{
    CompleteHooks, DuringCallHook, Error, ExecuteOperation, FailureHook, JsonExecution, NoHooks,
    OperationCodec, Pipeline, PostCallHook, PreCallHook, ResolveEndpoint, SuccessHook,
};
pub use plan::{
    AzureAi, AzureDocumentIntelligenceAnalyze, AzureDocumentIntelligencePlan, AzureMistralPlan,
    DocumentIntelligenceTransformation, Mistral, MistralOcr, MistralOcrPlan, MistralTransformation,
    Reducto, ReductoLegacyPlan, ReductoLegacyTransformation, ReductoParse, ReductoParseLegacy,
    ReductoV3Plan, ReductoV3Transformation, azure_document_intelligence, azure_mistral, mistral,
    reducto_legacy, reducto_v3,
};
pub use registry::{OcrComposition, dispatch, resolve_composition};
pub use types::{Ocr, OcrCall, OcrCallContext, OcrDocument, OcrPage, OcrRequest, OcrResponse};
