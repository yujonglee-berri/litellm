use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::types::RealtimeEvent;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RealtimeUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RealtimeMetadata {
    pub user_api_key_hash: Option<String>,
    pub user_api_key_user_id: Option<String>,
    pub user_api_key_team_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealtimeLogPayload {
    pub id: String,
    pub litellm_call_id: String,
    pub model: String,
    pub custom_llm_provider: String,
    pub usage: RealtimeUsage,
    pub response_cost: f64,
    pub start_time: f64,
    pub end_time: f64,
}

pub struct RealtimeInstrumentation {
    id: String,
    litellm_call_id: String,
    model: String,
    usage: RealtimeUsage,
    response_cost: f64,
    start_time: f64,
    end_time: f64,
}

impl RealtimeInstrumentation {
    pub fn new(litellm_call_id: impl Into<String>, model: impl Into<String>) -> Self {
        let litellm_call_id = litellm_call_id.into();
        let now = epoch_seconds();
        Self {
            id: litellm_call_id.clone(),
            litellm_call_id,
            model: model.into(),
            usage: RealtimeUsage::default(),
            response_cost: 0.0,
            start_time: now,
            end_time: now,
        }
    }

    pub fn observe(&mut self, event: &RealtimeEvent) {
        match event.event_type.as_str() {
            "session.created" | "session.updated" => self.observe_session(event),
            "response.done" => self.observe_response_done(event),
            _ => {}
        }
    }

    pub fn finish(&mut self) {
        self.end_time = epoch_seconds();
    }

    pub fn set_response_cost(&mut self, cost: f64) {
        self.response_cost = cost;
    }

    pub fn payload(&self) -> RealtimeLogPayload {
        RealtimeLogPayload {
            id: self.id.clone(),
            litellm_call_id: self.litellm_call_id.clone(),
            model: self.model.clone(),
            custom_llm_provider: "openai".to_string(),
            usage: self.usage.clone(),
            response_cost: self.response_cost,
            start_time: self.start_time,
            end_time: self.end_time,
        }
    }

    fn observe_session(&mut self, event: &RealtimeEvent) {
        let session = event.data.get("session").and_then(Value::as_object);
        if let Some(id) = session
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            self.id = id.to_string();
            self.litellm_call_id = id.to_string();
        }
        if let Some(model) = session
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            self.model = model.to_string();
        }
    }

    fn observe_response_done(&mut self, event: &RealtimeEvent) {
        let Some(usage) = event
            .data
            .get("response")
            .and_then(Value::as_object)
            .and_then(|response| response.get("usage"))
            .and_then(Value::as_object)
        else {
            return;
        };
        let input = usage.get("input_tokens").and_then(Value::as_u64);
        let output = usage.get("output_tokens").and_then(Value::as_u64);
        self.usage.prompt_tokens += input.unwrap_or(0);
        self.usage.completion_tokens += output.unwrap_or(0);
        self.usage.total_tokens += usage
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| input.unwrap_or(0) + output.unwrap_or(0));
    }
}

fn epoch_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(raw: &str) -> RealtimeEvent {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn observes_openai_session_identity_and_usage() {
        let mut instrumentation = RealtimeInstrumentation::new("fallback", "requested");
        instrumentation.observe(&event(
            r#"{"type":"session.created","session":{"id":"sess_1","model":"resolved"}}"#,
        ));
        instrumentation.observe(&event(
            r#"{"type":"response.done","response":{"usage":{"input_tokens":3,"output_tokens":2}}}"#,
        ));

        let payload = instrumentation.payload();
        assert_eq!(payload.id, "sess_1");
        assert_eq!(payload.litellm_call_id, "sess_1");
        assert_eq!(payload.model, "resolved");
        assert_eq!(payload.usage.prompt_tokens, 3);
        assert_eq!(payload.usage.completion_tokens, 2);
        assert_eq!(payload.usage.total_tokens, 5);
    }
}
