//! Crate-level constants for the inference gateway.
//!
//! Per `litellm-rust/CLAUDE.md`, magic numbers and fixed strings live here
//! (the Rust mirror of Python's `litellm/constants.py`), not inline in feature
//! modules. Env-overridable tunables keep their `DEFAULT_*` value here; the env
//! read + fallback happens at the host/config layer.

#[cfg(feature = "server")]
pub const PORTING_GET_ROUTES: &[&str] = &["/models", "/v1/models"];

#[cfg(feature = "server")]
pub const PORTING_POST_ROUTES: &[&str] = &[
    "/chat/completions",
    "/v1/chat/completions",
    "/completions",
    "/v1/completions",
    "/embeddings",
    "/v1/embeddings",
    "/moderations",
    "/v1/moderations",
    "/audio/transcriptions",
    "/v1/audio/transcriptions",
    "/audio/speech",
    "/v1/audio/speech",
    "/images/generations",
    "/v1/images/generations",
    "/rerank",
    "/v1/rerank",
    "/v2/rerank",
    "/ocr",
    "/v1/ocr",
    "/search",
    "/v1/search",
    "/v1/messages/count_tokens",
];

#[cfg(feature = "server")]
pub const PORTING_PASSTHROUGH_ROUTES: &[&str] = &[
    "/anthropic/*path",
    "/assemblyai/*path",
    "/azure/*path",
    "/azure_ai/*path",
    "/bedrock/*path",
    "/cohere/*path",
    "/gemini/*path",
    "/google/*path",
    "/groq/*path",
    "/mistral/*path",
    "/openai_passthrough/*path",
    "/vertex_ai/*path",
    "/vllm/*path",
    "/voyage/*path",
];

/// Default LiteLLM control-plane base URL for request-log egress when
/// `LITELLM_PROXY_BASE_URL` is unset.
pub(crate) const DEFAULT_PROXY_BASE_URL: &str = "http://localhost:4000";

/// The logs ingest path appended to the proxy base. Not a tunable; it is the
/// proxy's API contract (the rust-control-plane router on the Python proxy).
pub(crate) const RUST_CONTROL_PLANE_LOGS_PATH: &str = "/v1/rust_control_plane/logs";

/// Default bounded channel depth for the log-egress worker.
/// Override: `LITELLM_LOG_CHANNEL_CAPACITY`.
pub(crate) const DEFAULT_CHANNEL_CAPACITY: usize = 4096;

/// Default max records POSTed per request to the control plane.
/// Override: `LITELLM_LOG_BATCH_SIZE`.
pub(crate) const DEFAULT_MAX_BATCH_SIZE: usize = 256;

/// Default partial-batch flush cadence, in ms.
/// Override: `LITELLM_LOG_FLUSH_INTERVAL_MS`.
pub(crate) const DEFAULT_FLUSH_INTERVAL_MS: u64 = 500;

/// HTTP path for the non-streaming Anthropic Messages route.
#[cfg(feature = "server")]
pub(crate) const MESSAGES_ROUTE_PATH: &str = "/v1/messages";

/// Request headers owned by the gateway and never forwarded upstream.
#[cfg(feature = "server")]
pub(crate) const MESSAGES_HEADERS_NOT_FORWARDED: &[&str] =
    &["authorization", "connection", "content-length", "host"];
