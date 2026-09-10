# Shared outbound authentication

`Auth` authenticates the final `reqwest::Request` asynchronously and returns the request to send. The implementation owns its injected dependencies. It can inspect the HTTP method, resolved URL and query, headers, and serialized body. It is object safe, so routes can accept `Arc<dyn Auth>` without knowing the provider or credential mechanism

`HeaderAuth` supports API keys, bearer tokens, and arbitrary sets of credential headers. `BearerTokenAuth` calls the existing `TokenProvider` for every attempt, allowing that provider to own expiry, refresh, and caching. Both handle existing headers explicitly and mark credential values sensitive. `NoAuth` covers unauthenticated upstreams

The earlier `Authenticate<Request, Services>` contract is a preparation contract: it returns headers and endpoint context before a wire request exists. OCR's current implementations mix that preparation with environment lookup and provider error mapping. Keep route input parsing and credential precedence in preparation, move reusable acquisition into `auth`, then apply `Auth` after endpoint resolution, serialization, and body-changing hooks. Header-only preparation cannot implement request signing

## Python coverage and remaining adapters

Paths below are relative to the repository root. This inventory describes the current Python checkout, not a claim that Rust implements every mechanism

| Mechanism | Python reference | Shared Rust responsibility |
| --- | --- | --- |
| Static API keys and bearer tokens | `litellm/llms/anthropic/common_utils.py`, `litellm/llms/azure_ai/common_utils.py`, `litellm/llms/gemini/common_utils.py` | Header application is implemented. Provider preparation chooses header names and precedence |
| Named credentials and secret references | `litellm/litellm_core_utils/credential_accessor.py` | Existing `CredentialResolver` resolves host-owned references |
| Azure Entra and OIDC | `litellm/llms/azure/common_utils.py`, `litellm/secret_managers/get_azure_ad_token_provider.py` | Existing `auth/azure` acquisition can feed a token provider. Preserve source and destination checks |
| Google ADC, service accounts, external identity, AWS federation | `litellm/llms/vertex_ai/vertex_llm_base.py`, `litellm/llms/vertex_ai/vertex_ai_aws_wif.py` | Add SDK-backed token providers with scope/project-aware refresh |
| OpenAI workload identity | `litellm/llms/openai/workload_identity.py` | Add an SDK-backed provider, preserving endpoint eligibility and static-key precedence |
| AWS keys, profiles, ambient chain, STS role and web identity | `litellm/llms/bedrock/base_aws_llm.py` | Credential material includes access key, secret, and session token. Do not squeeze it into `ResolvedCredential::AccessToken` |
| AWS SigV4 | `litellm/llms/bedrock/base_aws_llm.py`, `litellm/llms/sagemaker/chat/handler.py` | Adapt existing Rust AWS signing behind `Auth`, parameterized by service/region and injected clock. Sign the exact final bytes |
| OCI and Volcengine signatures | `litellm/llms/oci/common_utils.py`, `litellm/llms/volcengine/` | SDK-backed request authenticators with provider-specific credential types |
| OAuth exchanges, device login, IAM tokens | `litellm/llms/github_copilot/authenticator.py`, `litellm/llms/gigachat/authenticator.py`, `litellm/llms/xai/oauth.py`, `litellm/llms/sap/credentials.py` | Add token providers. Interactive login and disk persistence belong in the host |
| SDK unified auth and IAM API-key exchange | `litellm/llms/databricks/common_utils.py`, `litellm/llms/watsonx/common_utils.py` | Adapt SDK-produced headers through `Auth`; token-only exchanges can use `TokenProvider` |
| Query credentials | `litellm/llms/gemini/` | Add a URL-mutating authenticator with explicit duplicate-key handling and encoding. URL logging needs redaction beyond sensitive headers |
| Basic auth, configurable prefixes, passthrough, delegated exchanges | `litellm/proxy/_experimental/mcp_server/outbound_credentials/types.py`, `litellm/proxy/_experimental/mcp_server/outbound_credentials/httpx_auth.py` | Reuse header application; add acquisition for client credentials, authorization code, token exchange and OBO as needed. Some Python resolver variants are declarations or stubs |

## Execution rules

Acquire credentials through injected providers. Let SDKs manage refresh where available. Cache by credential identity and relevant tenant/user, scope/audience and cloud configuration, with expiry and single-flight refresh. A failed acquisition is terminal unless a provider explicitly defines a fallback policy

Bind host credentials to trusted destinations before authentication. Preserve existing source checks when moving Azure code. The trait itself does not authorize an arbitrary URL. Rebuild and authenticate every retry and polling request against its actual method, URL, and body. Do not reuse signatures across requests or automatically forward credentials across redirects. Signers must reject unsupported streaming bodies or implement the provider's streaming-signature protocol

Apply authentication after guardrails and other body/URL mutations. Do not mutate signed fields afterward. Sensitive header values protect Rust debug output, but callers must also redact any explicit header extraction, query credentials, and SDK errors

TLS trust, client certificates, and proxy settings belong to the reused HTTP client. Inbound virtual-key/JWT validation, principals, budgets, and RBAC belong to gateway authorization. Neither belongs in outbound `Auth`
