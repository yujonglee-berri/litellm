- every crates's lib.rs should be thin entrypoint.
- for crates's error. use error.rs with thiserror, single top-level Error enum.

# Crates

The workspace has 30 crates

| Crate | Role |
|-------|------|
| litellm-auth | Outbound authentication contracts |
| litellm-auth-azure | Azure authentication |
| litellm-auth-aws | AWS authentication |
| litellm-auth-google | Google authentication |
| litellm-auth-oauth | OAuth authentication |
| litellm-config | Gateway configuration |
| litellm-core | Existing SDK and provider implementation pending migration |
| litellm-lifecycle | Operation-independent lifecycle contracts |
| litellm-operation | Provider-independent semantic operation types and pipeline composition |
| litellm-operation-audio-transcription | Audio transcription semantic contract |
| litellm-operation-chat-completions | Chat Completions semantic contract and plans |
| litellm-operation-messages | Messages semantic contract and Anthropic Messages wire adapter |
| litellm-operation-ocr | OCR semantic contract and plans |
| litellm-operation-realtime | Realtime semantic contract, OpenAI WebSocket execution, instrumentation, and warm-session runtime |
| litellm-operation-responses | Responses semantic contract |
| litellm-protocol | Upstream wire contracts and protocol decoding |
| litellm-adapters | Semantic transformations between caller and upstream contracts |
| litellm-pipeline | Typed execution and lifecycle orchestration |
| litellm-providers | Provider presets and composition entry points |
| litellm-transport | Final-request authentication and execution boundary |
| litellm-gateway-agent | Agent gateway routes |
| litellm-gateway-auth | Inbound gateway authentication |
| litellm-gateway-inference | Inference gateway routes |
| litellm-gateway-management | Management gateway routes |
| litellm-gateway-otel | Optional OpenTelemetry instrumentation and export |
| litellm-gateway-router | Gateway routing |
| litellm-gateway-server | Combined gateway host |
| litellm-persist | Persistence backends injected by the gateway host |
| litellm-python-bridge | Python extension module |
| litellm-python-interop | Python interop foundation |

Auth crates must not depend on operation crates. Protocol depends on `litellm-operation`; adapters connect operation crates to protocol; pipeline depends on shared foundations; providers assemble concrete adapters, pipeline execution, auth, endpoints, and operation contracts. New caller operation code must not depend on protocol, adapters, pipeline, providers, core, or gateway crates
