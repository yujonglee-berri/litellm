use axum::http::Method;

const PASSTHROUGH_PREFIXES: &[&str] = &[
    "anthropic",
    "assemblyai",
    "azure",
    "azure_ai",
    "bedrock",
    "cohere",
    "cursor",
    "eu.assemblyai",
    "gemini",
    "milvus",
    "mistral",
    "openai",
    "openai_passthrough",
    "vertex-ai",
    "vertex_ai",
    "vllm",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    path: String,
    template: Option<String>,
}

impl Route {
    pub fn new(path: impl Into<String>, template: Option<impl Into<String>>) -> Self {
        Self {
            path: path.into(),
            template: template.map(Into::into),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    pub fn telemetry_path(&self) -> &str {
        if self.is_passthrough() {
            return &self.path;
        }

        self.template.as_deref().unwrap_or(&self.path)
    }

    pub fn is_passthrough(&self) -> bool {
        let first_segment = self.path.trim_start_matches('/').split('/').next();
        first_segment.is_some_and(|prefix| PASSTHROUGH_PREFIXES.contains(&prefix))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteContext {
    method: Method,
    route: Route,
}

impl RouteContext {
    pub fn new(method: Method, route: Route) -> Self {
        Self { method, route }
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn route(&self) -> &Route {
        &self.route
    }

    pub fn span_name(&self) -> String {
        format!("{} {}", self.method, self.route.telemetry_path())
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Method;

    use super::{Route, RouteContext};

    #[test]
    fn normal_route_uses_low_cardinality_template() {
        let context = RouteContext::new(
            Method::GET,
            Route::new(
                "/v1/threads/thread-123/runs",
                Some("/v1/threads/{thread_id}/runs"),
            ),
        );

        assert_eq!(context.span_name(), "GET /v1/threads/{thread_id}/runs");
    }

    #[test]
    fn passthrough_route_uses_literal_path() {
        let context = RouteContext::new(
            Method::POST,
            Route::new("/openai/v1/chat/completions", Some("/openai/{endpoint}")),
        );

        assert_eq!(context.span_name(), "POST /openai/v1/chat/completions");
    }
}
