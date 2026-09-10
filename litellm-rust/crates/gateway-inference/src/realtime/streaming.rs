use std::sync::Arc;

use litellm_core::realtime::instrumentation::{RealtimeInstrumentation, RealtimeLogPayload};
use litellm_core::realtime::types::RealtimeEvent;

use crate::integrations::custom_logger::{
    CallbackTiming, CallbackValue, CustomLogger, CustomLoggerRunner, LoggingError, ModelCallDetails,
};
use crate::integrations::types::{
    RequestMetadata, StandardLoggingMetadata, StandardLoggingPayload,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Success,
    Failure,
}

pub struct RealTimeStreaming {
    callbacks: Vec<Arc<dyn CustomLogger>>,
    instrumentation: RealtimeInstrumentation,
    metadata: RequestMetadata,
    dropped: u64,
}

impl RealTimeStreaming {
    pub fn new(
        callbacks: Vec<Arc<dyn CustomLogger>>,
        litellm_call_id: String,
        model: String,
        metadata: RequestMetadata,
    ) -> Self {
        Self {
            callbacks,
            instrumentation: RealtimeInstrumentation::new(litellm_call_id, model),
            metadata,
            dropped: 0,
        }
    }

    pub fn observe(&mut self, event: &RealtimeEvent) {
        self.instrumentation.observe(event);
    }

    pub async fn log_messages(&mut self, status: SessionStatus) {
        self.instrumentation.finish();
        let payload = self.build_payload();
        let timing = CallbackTiming::new(payload.start_time, payload.end_time);
        let runner = CustomLoggerRunner::new(self.callbacks.clone());
        let report = match status {
            SessionStatus::Success => {
                let response = CallbackValue::new("realtime", serde_json::Value::Null);
                runner
                    .async_log_success_event(
                        &ModelCallDetails::from_standard_logging_payload(payload),
                        &response,
                        timing,
                    )
                    .await
            }
            SessionStatus::Failure => {
                let error = LoggingError {
                    message: "realtime session ended in failure".to_string(),
                    kind: "RealtimeSessionError".to_string(),
                };
                let response = CallbackValue::new(
                    "error",
                    serde_json::json!({"message": error.message, "kind": error.kind}),
                );
                runner
                    .async_log_failure_event(
                        &ModelCallDetails::from_standard_logging_payload(payload)
                            .with_failure_error(error),
                        Some(&response),
                        timing,
                    )
                    .await
            }
        };
        self.dropped += report.dropped as u64;
    }

    fn build_payload(&self) -> StandardLoggingPayload {
        let RealtimeLogPayload {
            id,
            litellm_call_id,
            model,
            custom_llm_provider,
            usage,
            response_cost,
            start_time,
            end_time,
        } = self.instrumentation.payload();
        StandardLoggingPayload {
            id,
            litellm_call_id,
            call_type: "realtime".to_string(),
            model,
            custom_llm_provider,
            response_cost,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            start_time,
            end_time,
            stream: true,
            metadata: StandardLoggingMetadata {
                user_api_key_hash: self.metadata.user_api_key_hash.clone(),
                user_api_key_user_id: self.metadata.user_api_key_user_id.clone(),
                user_api_key_team_id: self.metadata.user_api_key_team_id.clone(),
                ..Default::default()
            },
            messages: None,
        }
    }
}
