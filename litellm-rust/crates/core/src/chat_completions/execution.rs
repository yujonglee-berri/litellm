use serde_json::Value;

use crate::auth::Auth;
use crate::error::Error;
use crate::http_utils::{http_request, truncate_error_body};

use super::client::http_client;
use super::transformations::transformation_for;
use super::types::{
    ChatCompletionsResponse, ChatCompletionsTransformResponse, ProviderChatCompletionsRequest,
    ProviderChatResponseData,
};
use crate::operation::OperationPlan;

#[tracing::instrument(target = "litellm::function_trace", level = "trace", skip_all)]
pub(super) async fn execute_chat_completions_provider_call(
    request: ProviderChatCompletionsRequest,
) -> Result<ChatCompletionsResponse, Error> {
    let body = serde_json::to_vec(&request.body).map_err(|err| {
        Error::InvalidRequest(format!(
            "failed to serialize chat completions request: {err}"
        ))
    })?;
    let client = http_client().clone();
    let mut request_builder = client.post(&request.url).body(body);
    for (key, value) in &request.upstream_headers {
        request_builder = request_builder.header(key, value);
    }
    if let Some(duration) = request.timeout {
        request_builder = request_builder.timeout(duration);
    }

    let wire_request = request_builder
        .build()
        .map_err(|error| Error::InvalidRequest(error.without_url().to_string()))?;
    let wire_request = request
        .auth
        .authenticate(wire_request)
        .await
        .map_err(auth_error)?;
    let response = http_request(reqwest::RequestBuilder::from_parts(client, wire_request))
        .await
        .map_err(|err| {
            // Failing to establish the connection means the request never went out,
            // so the host can still serve it. Everything else here, a timeout
            // above all, may have reached the provider and been answered.
            if err.is_connect() || err.is_builder() {
                Error::Connect(err.to_string())
            } else {
                Error::Network(err.to_string())
            }
        })?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| Error::Network(err.to_string()))?;

    if !status.is_success() {
        return Err(Error::Http {
            status: status.as_u16(),
            body: truncate_error_body(&text),
        });
    }

    let body: Value = serde_json::from_str(&text).map_err(|err| {
        Error::InvalidResponse(format!("invalid chat completions response JSON: {err}"))
    })?;
    transformation_for(request.plan.transformation())
        .transform_response(ChatCompletionsTransformResponse {
            model: request.model,
            response: ProviderChatResponseData { body },
        })
        .map_err(as_response_error)
}

/// Re-tag an error raised while normalizing a response the provider already
/// returned.
///
/// A transformation reports the same variants on either side of the call: a missing
/// field or an unsupported block can mean "this request cannot be translated"
/// during prepare and "this response cannot be normalized" here. Only the
/// second kind has already been billed, and a host that keeps a reference
/// implementation must not retry those, so collapse them to one variant that
/// can only mean the provider was already called.
pub(super) fn as_response_error(err: Error) -> Error {
    match err {
        already @ (Error::InvalidResponse(_) | Error::Http { .. }) => already,
        other => Error::InvalidResponse(other.to_string()),
    }
}

pub(super) fn auth_error(error: crate::AuthError) -> Error {
    if matches!(
        error,
        crate::AuthError::Aws(crate::auth::error::AwsAuthError::ComputedHeader(_))
    ) {
        Error::Unsupported("request forwards a header AWS SigV4 computes")
    } else {
        Error::from(error)
    }
}

#[cfg(all(test, feature = "bedrock-auth"))]
pub(super) async fn signed_headers(
    request: &ProviderChatCompletionsRequest,
    body: &[u8],
) -> Result<Vec<(String, String)>, Error> {
    let client = http_client().clone();
    let mut builder = client.post(&request.url).body(body.to_vec());
    for (name, value) in &request.upstream_headers {
        builder = builder.header(name, value);
    }
    let wire = builder
        .build()
        .map_err(|error| Error::InvalidRequest(error.without_url().to_string()))?;
    let authenticated = request.auth.authenticate(wire).await.map_err(auth_error)?;
    authenticated
        .headers()
        .iter()
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.as_str().to_string(), value.to_string()))
                .map_err(|_| Error::Auth("authenticated header is not text".into()))
        })
        .collect()
}

#[cfg(all(test, not(feature = "bedrock-auth")))]
pub(super) async fn signed_headers(
    request: &ProviderChatCompletionsRequest,
    body: &[u8],
) -> Result<Vec<(String, String)>, Error> {
    let client = http_client().clone();
    let mut builder = client.post(&request.url).body(body.to_vec());
    for (name, value) in &request.upstream_headers {
        builder = builder.header(name, value);
    }
    let wire = builder
        .build()
        .map_err(|error| Error::InvalidRequest(error.without_url().to_string()))?;
    let authenticated = request.auth.authenticate(wire).await.map_err(auth_error)?;
    authenticated
        .headers()
        .iter()
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.as_str().to_string(), value.to_string()))
                .map_err(|_| Error::Auth("authenticated header is not text".into()))
        })
        .collect()
}
