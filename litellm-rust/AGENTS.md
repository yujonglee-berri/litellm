# AGENTS.md

litellm-rust has thirteen crates. A crate is a layer or shared foundation, not a route. Routes (ocr, realtime, chat) and providers (mistral, openai) are modules inside the layers. Authentication mechanisms are separate foundations because their cloud SDK dependency trees must not inflate every core build. The inference and management gateways are separate deployable artifacts.

## Crates

| Crate | Role |
|-------|------|
| litellm-auth | Operation-independent credential, token-provider, and final-request authentication contracts. |
| litellm-auth-aws | AWS credential-chain, STS, and SigV4 implementation. |
| litellm-auth-azure | Azure Entra credential selection and token acquisition. |
| litellm-auth-google | Typed Google access-token, service-account, ADC, and external-account/WIF credential sources. |
| litellm-auth-oauth | Typed OAuth grant exchanges; interactive login and durable token storage stay in the host. |
| litellm-core | The LiteLLM SDK in Rust. One public entrypoint per top-level call (`messages::messages()`), owning types, transforms, provider resolution, auth, and the provider HTTP call. Call it, get a typed response. |
| litellm-config | Config-loading boundary. Returns resolved gateway-router deployment data and optionally delegates loading to Python. |
| litellm-gateway-inference | The inference axum server and WebSocket hosts. Registers only data-plane routes and translates them to core entrypoints. |
| litellm-gateway-management | The management axum server scaffold. Management and control-plane routes belong here. |
| litellm-gateway-auth | Inbound gateway authorization and credential verification; separate from outbound provider authentication. |
| litellm-gateway-router | Gateway deployment selection and routing policy. |
| litellm-python-interop | Domain-neutral PyO3 foundation for GIL handling and typed Python/Serde conversion. |
| litellm-python-bridge | PyO3 cdylib exposing LiteLLM Rust APIs to the Python SDK. Owns API registration, domain wiring, and Python exception mapping. |

Dependency direction is acyclic: cloud auth crates depend on `litellm-auth`; `litellm-core` depends on the auth crates; `litellm-config` depends on `litellm-gateway-router`; the inference gateway depends on config, core, auth, and router; and `litellm-python-bridge` depends on the domain layers and `litellm-python-interop`. Auth, router, management gateway, and interop foundations depend on no LiteLLM domain crate.

## Where a route lives

A top-level LiteLLM call is a module under `crates/core/src/<route>/`, shaped like `messages`:

```
core/src/messages/
  mod.rs             # pub async fn messages(..) -> CoreResult<..>  (+ messages_stream for SSE)
  types.rs           # request/response types, MessagesRequest
  transformation.rs  # the provider template trait
  prepare.rs         # provider resolution, auth headers, URL
  handler.rs         # the provider call
  client.rs          # the shared reqwest client
```

Handlers never live in `gateway-inference`. `ocr`, `audio_transcription`, and `realtime` are still hosted there from before this rule; they move to `core` as they are touched.

Adding a crate: default to a module. A new crate requires a real trigger: separate artifact (binary/cdylib), proc-macro, shared foundation, or publishable standalone. A new provider or route is none of these.

Adding a crate fails crates/core/tests/workspace_crate_allowlist.rs until you update its allowlist and this file — intentional.

## Style

All Rust in `litellm-rust/` follows the official Rust Style Guide:
https://doc.rust-lang.org/style-guide/

`rustfmt` implements its formatting by default, so run `cargo fmt` before committing; CI gates every PR on `cargo fmt --check`. Do not hand-format against rustfmt or add a `rustfmt.toml` that diverges from the default style.

Beyond formatting, follow the guide's naming and idiom conventions rustfmt cannot auto-apply: `snake_case` items/functions/modules, `UpperCamelCase` types/traits/variants, `SCREAMING_SNAKE_CASE` constants/statics (acronyms as one word, e.g. `HttpClient`), and the import grouping and item ordering it prescribes. See CLAUDE.md for the detailed version.
