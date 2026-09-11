use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResponsesSessionEventType {
    ResponseCreate,
    ResponseCreated,
    ResponseCompleted,
    ResponseFailed,
    ResponseIncomplete,
    Error,
    Other(String),
}

impl ResponsesSessionEventType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ResponseCreate => "response.create",
            Self::ResponseCreated => "response.created",
            Self::ResponseCompleted => "response.completed",
            Self::ResponseFailed => "response.failed",
            Self::ResponseIncomplete => "response.incomplete",
            Self::Error => "error",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for ResponsesSessionEventType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ResponsesSessionEventType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "response.create" => Self::ResponseCreate,
            "response.created" => Self::ResponseCreated,
            "response.completed" => Self::ResponseCompleted,
            "response.failed" => Self::ResponseFailed,
            "response.incomplete" => Self::ResponseIncomplete,
            "error" => Self::Error,
            _ => Self::Other(value),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsesSessionEvent {
    #[serde(rename = "type")]
    pub event_type: ResponsesSessionEventType,
    #[serde(flatten)]
    pub data: Map<String, Value>,
}

impl ResponsesSessionEvent {
    pub fn model(&self) -> Option<&str> {
        let model = self.data.get("model").and_then(Value::as_str);
        if model.is_some() {
            return model;
        }
        self.data
            .get("response")
            .and_then(Value::as_object)
            .and_then(|response| response.get("model"))
            .and_then(Value::as_str)
    }

    pub fn is_response_create(&self) -> bool {
        self.event_type == ResponsesSessionEventType::ResponseCreate
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsesSessionTransformResult {
    pub events: Vec<ResponsesSessionEvent>,
}

impl ResponsesSessionTransformResult {
    pub fn passthrough(event: ResponsesSessionEvent) -> Self {
        Self {
            events: vec![event],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_round_trips_known_and_unknown_values() {
        let known: ResponsesSessionEventType =
            serde_json::from_str("\"response.completed\"").expect("valid event type");
        assert_eq!(known, ResponsesSessionEventType::ResponseCompleted);
        let unknown: ResponsesSessionEventType =
            serde_json::from_str("\"response.output_text.delta\"").expect("valid event type");
        assert_eq!(
            unknown,
            ResponsesSessionEventType::Other("response.output_text.delta".to_string())
        );
    }

    #[test]
    fn model_reads_flat_and_nested_create_shapes() {
        let flat: ResponsesSessionEvent =
            serde_json::from_value(serde_json::json!({"type":"response.create","model":"gpt-5"}))
                .expect("valid event");
        let nested: ResponsesSessionEvent = serde_json::from_value(serde_json::json!({
            "type":"response.create",
            "response":{"model":"gpt-5-mini"}
        }))
        .expect("valid event");
        assert_eq!(flat.model(), Some("gpt-5"));
        assert_eq!(nested.model(), Some("gpt-5-mini"));
    }
}
