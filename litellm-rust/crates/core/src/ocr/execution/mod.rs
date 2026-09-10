mod polling;
mod reducto;

use std::future::Future;

use crate::auth::Auth;

use super::OcrClient;
use super::auth::ResolvedOcrAuth;
use super::codecs::document_intelligence::DocumentIntelligenceCodec;
use super::codecs::mistral::{MistralOcrCodec, MistralOcrRequest};
use super::codecs::{DecodeOcrResponse, EncodeOcrRequest};
use super::document::{inline_remote_document, validate_inline_document};
use super::error::OcrError;
use super::prepare::transform_request_body;
use super::types::{LiteLLMOcrRequest, LiteLLMOcrResponse, OcrResponseFormat};
use super::wire::DecodedOcrResponse;

pub(crate) use reducto::ReductoExecution;

pub(crate) trait ExecuteOcr<C, A, AuthContext>: Send + Sync
where
    C: EncodeOcrRequest + DecodeOcrResponse,
    A: Auth,
    AuthContext: Send + Sync,
{
    fn execute(
        &self,
        client: &OcrClient,
        codec: &C,
        request: &LiteLLMOcrRequest,
        params: &C::Params,
        endpoint: &str,
        authentication: &ResolvedOcrAuth<A, AuthContext>,
    ) -> impl Future<Output = Result<LiteLLMOcrResponse, OcrError>> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct JsonExecution;

impl<C, A, AuthContext> ExecuteOcr<C, A, AuthContext> for JsonExecution
where
    C: EncodeOcrRequest + DecodeOcrResponse,
    A: Auth,
    AuthContext: Send + Sync,
{
    async fn execute(
        &self,
        client: &OcrClient,
        codec: &C,
        request: &LiteLLMOcrRequest,
        params: &C::Params,
        endpoint: &str,
        authentication: &ResolvedOcrAuth<A, AuthContext>,
    ) -> Result<LiteLLMOcrResponse, OcrError> {
        let body = codec.encode(&request.model, request.document.clone(), params)?;
        let provider_request = transform_request_body(
            client,
            request,
            endpoint,
            &authentication.headers,
            body,
            |_| Ok(()),
        )
        .await?;
        let decoded = send_json::<C::WireResponse, A>(
            client,
            request,
            provider_request,
            &authentication.authenticator,
        )
        .await?;
        finish(codec, request, decoded)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InlineJsonExecution;

impl<A, AuthContext> ExecuteOcr<MistralOcrCodec, A, AuthContext> for InlineJsonExecution
where
    A: Auth,
    AuthContext: Send + Sync,
{
    async fn execute(
        &self,
        client: &OcrClient,
        codec: &MistralOcrCodec,
        request: &LiteLLMOcrRequest,
        params: &<MistralOcrCodec as EncodeOcrRequest>::Params,
        endpoint: &str,
        authentication: &ResolvedOcrAuth<A, AuthContext>,
    ) -> Result<LiteLLMOcrResponse, OcrError> {
        let document = inline_remote_document(
            client.document_fetcher(),
            request.document.clone(),
            &request.connection,
        )
        .await?;
        let body = codec.encode(&request.model, document, params)?;
        let provider_request = transform_request_body(
            client,
            request,
            endpoint,
            &authentication.headers,
            body,
            validate_mistral_document,
        )
        .await?;
        let decoded = send_json::<<MistralOcrCodec as DecodeOcrResponse>::WireResponse, A>(
            client,
            request,
            provider_request,
            &authentication.authenticator,
        )
        .await?;
        finish(codec, request, decoded)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DocumentIntelligenceExecution;

impl<A, AuthContext> ExecuteOcr<DocumentIntelligenceCodec, A, AuthContext>
    for DocumentIntelligenceExecution
where
    A: Auth,
    AuthContext: Send + Sync,
{
    async fn execute(
        &self,
        client: &OcrClient,
        codec: &DocumentIntelligenceCodec,
        request: &LiteLLMOcrRequest,
        params: &<DocumentIntelligenceCodec as EncodeOcrRequest>::Params,
        endpoint: &str,
        authentication: &ResolvedOcrAuth<A, AuthContext>,
    ) -> Result<LiteLLMOcrResponse, OcrError> {
        let body = codec.encode(&request.model, request.document.clone(), params)?;
        let provider_request = transform_request_body(
            client,
            request,
            endpoint,
            &authentication.headers,
            body,
            |_| Ok(()),
        )
        .await?;
        let response = send(client, provider_request, &authentication.authenticator).await?;
        let decoded = polling::read_operation_response(
            client.provider_http(),
            response,
            endpoint,
            &authentication.headers,
            &authentication.authenticator,
            &request.connection,
            request.response_format()? == OcrResponseFormat::Native,
        )
        .await?;
        finish::<DocumentIntelligenceCodec>(codec, request, decoded)
    }
}

async fn send_json<R, A>(
    client: &OcrClient,
    request: &LiteLLMOcrRequest,
    provider_request: reqwest::Request,
    auth: &A,
) -> Result<DecodedOcrResponse<R>, OcrError>
where
    R: serde::de::DeserializeOwned,
    A: Auth,
{
    let response = send(client, provider_request, auth).await?;
    crate::ocr::client::read_json_response(
        response,
        request.response_format()? == OcrResponseFormat::Native,
    )
    .await
}

async fn send<A: Auth>(
    client: &OcrClient,
    request: reqwest::Request,
    auth: &A,
) -> Result<reqwest::Response, OcrError> {
    let request = auth
        .authenticate(request)
        .await
        .map_err(crate::Error::from)?;
    crate::http_utils::http_request(reqwest::RequestBuilder::from_parts(
        client.provider_http().clone(),
        request,
    ))
    .await
    .map_err(crate::error::TransportError::from)
    .map_err(OcrError::from)
}

fn finish<C>(
    codec: &C,
    request: &LiteLLMOcrRequest,
    decoded: DecodedOcrResponse<C::WireResponse>,
) -> Result<LiteLLMOcrResponse, OcrError>
where
    C: DecodeOcrResponse,
{
    let response = codec.decode(&request.model, decoded.data)?;
    Ok(LiteLLMOcrResponse {
        provider_native_response: decoded.native,
        ..response
    })
}

fn validate_mistral_document(
    body: &MistralOcrRequest,
) -> Result<(), super::error::OcrRequestError> {
    validate_inline_document(&body.document)
}
