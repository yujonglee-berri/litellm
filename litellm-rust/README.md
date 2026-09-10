# LiteLLM Rust

This workspace contains the staged Rust implementation for LiteLLM.

`litellm-core` is the LiteLLM SDK in Rust: one entrypoint per top-level call
that makes the LLM call and hands back a typed response, the same shape as
`litellm.messages()` in Python.

```rust
let response = litellm_core::messages::messages(MessagesRequest {
    model: "claude-sonnet-4-5",
    body,
    api_key: Some(key),
    ..
})
.await?;
```

Python continues to own configuration, retries, routing policy, logging,
callbacks, spend tracking, and customer plugins until each Rust path has parity
coverage and production evidence.

## Crates

| Crate | Role |
|-------|------|
| litellm-core | The SDK. Per-route entrypoints (`messages::messages()`), types, provider transforms (modules under `providers/`), provider resolution, auth, and the provider HTTP call. |
| litellm-config | Config-loading boundary. Returns resolved deployments and optionally delegates loading to Python. |
| litellm-gateway-inference | The data-plane axum server and WebSocket hosts. Translates inference HTTP/WS routes to core entrypoints. |
| litellm-gateway-management | The control-plane axum server scaffold for management APIs. |
| litellm-gateway-router | Gateway deployment selection and routing policy. |
| litellm-python-interop | Domain-neutral PyO3 foundation for GIL handling and typed Python/Serde conversion. |
| litellm-python-bridge | PyO3 cdylib exposing LiteLLM Rust APIs to the Python SDK. Owns API registration, domain wiring, and Python exception mapping. |

Dependency direction is acyclic: config depends on gateway-router, gateway hosts depend on their shared foundations, and the Python bridge depends on the domain layers and Python interop.

## Layout

```text
crates/
  core/           The SDK: route modules + provider transforms.
    src/messages/   mod.rs (entrypoint), types, transformation, prepare, handler, client
    src/providers/anthropic/messages/transformation.rs
  config/         Config loading and resolved deployments.
  gateway-inference/  Data-plane axum server + WebSocket hosts; calls core entrypoints.
  gateway-management/ Control-plane axum server scaffold.
  gateway-router/ Gateway deployment selection and routing policy.
  python-interop/ Domain-neutral PyO3 conversion and GIL primitives.
  python-bridge/  PyO3 API adapter for Python LiteLLM.
```

The folder shape follows the Python provider tree:
`core/src/providers/<provider>/<route>/transformation.rs`. The bridge exposes one
function per top-level route, mirroring the core entrypoints.

## Checks

Run the commands under "Checks" in [CLAUDE.md](CLAUDE.md) before pushing Rust
changes. That list is the single source of truth and matches what GitHub Actions
runs for changes under `litellm-rust/`.
