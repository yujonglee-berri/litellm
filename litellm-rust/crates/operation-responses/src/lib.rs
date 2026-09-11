mod client;
mod error;
mod openai;
mod plan;
mod types;
mod wire;

pub use client::{ResponsesSessionConfig, resolve_model, responses_session};
pub use error::Error;
pub use openai::{
    OPENAI_RESPONSES_CONFIG, OPENAI_RESPONSES_DEFAULT_API_BASE, OPENAI_RESPONSES_PATH,
    OpenAIResponsesConfig, complete_websocket_url, enforce_model,
};
pub use plan::{
    OpenAI, OpenAICompletePlan, OpenAIResponses, OpenAIResponsesTransformation, OpenAISessionPlan,
    OpenAIStreamPlan, openai_complete, openai_session, openai_stream,
};
pub use types::{Responses, ResponsesRequest, ResponsesResponse, ResponsesStreamEvent};
pub use wire::{ResponsesSessionEvent, ResponsesSessionEventType, ResponsesSessionTransformResult};
