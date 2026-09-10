mod polling;
mod reducto;

use std::future::Future;

use crate::auth::Auth;

use super::OcrClient;
use super::document::{inline_remote_document, validate_inline_document};
use super::error::OcrError;
use super::prepare::transform_request_body;
use super::transformations::document_intelligence::DocumentIntelligenceTransformation;
use super::transformations::mistral::{MistralOcrRequest, MistralOcrTransformation};
use super::transformations::{OcrTransformRequest, OcrTransformResponse, OcrTransformation};
use super::types::{LiteLLMOcrRequest, LiteLLMOcrResponse, OcrResponseFormat};
use super::wire::DecodedOcrResponse;
use crate::auth::ResolvedAuth;
use crate::operation::RequestTransformation;

pub(crate) use reducto::ReductoExecution;

pub(crate) trait ExecuteOcr<D, A, AuthContext>: Send + Sync
where
    D: OcrTransformation,
    A: Auth,
    AuthContext: Send + Sync,
{
    fn execute(
        &self,
        client: &OcrClient,
        transformation: &D,
        request: &LiteLLMOcrRequest,
        params: &D::Params,
        endpoint: &str,
        authentication: &ResolvedAuth<A, AuthContext>,
    ) -> impl Future<Output = Result<LiteLLMOcrResponse, OcrError>> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct JsonExecution;

impl<D, A, AuthContext> ExecuteOcr<D, A, AuthContext> for JsonExecution
where
    D: OcrTransformation,
    A: Auth,
    AuthContext: Send + Sync,
{
    async fn execute(
        &self,
        client: &OcrClient,
        transformation: &D,
        request: &LiteLLMOcrRequest,
        params: &D::Params,
        endpoint: &str,
        authentication: &ResolvedAuth<A, AuthContext>,
    ) -> Result<LiteLLMOcrResponse, OcrError> {
        let body = transformation.transform_request(OcrTransformRequest {
            model: request.model.clone(),
            document: request.document.clone(),
            params: params.clone(),
        })?;
        let provider_request = transform_request_body(
            client,
            request,
            endpoint,
            &authentication.headers,
            body,
            |_| Ok(()),
        )
        .await?;
        let decoded = send_json::<D::WireResponse, A>(
            client,
            request,
            provider_request,
            &authentication.authenticator,
        )
        .await?;
        finish(transformation, request, decoded)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InlineJsonExecution;

impl<A, AuthContext> ExecuteOcr<MistralOcrTransformation, A, AuthContext> for InlineJsonExecution
where
    A: Auth,
    AuthContext: Send + Sync,
{
    async fn execute(
        &self,
        client: &OcrClient,
        transformation: &MistralOcrTransformation,
        request: &LiteLLMOcrRequest,
        params: &<MistralOcrTransformation as OcrTransformation>::Params,
        endpoint: &str,
        authentication: &ResolvedAuth<A, AuthContext>,
    ) -> Result<LiteLLMOcrResponse, OcrError> {
        let document = inline_remote_document(
            client.document_fetcher(),
            request.document.clone(),
            &request.connection,
        )
        .await?;
        let body = transformation.transform_request(OcrTransformRequest {
            model: request.model.clone(),
            document,
            params: params.clone(),
        })?;
        let provider_request = transform_request_body(
            client,
            request,
            endpoint,
            &authentication.headers,
            body,
            validate_mistral_document,
        )
        .await?;
        let decoded =
            send_json::<<MistralOcrTransformation as OcrTransformation>::WireResponse, A>(
                client,
                request,
                provider_request,
                &authentication.authenticator,
            )
            .await?;
        finish(transformation, request, decoded)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DocumentIntelligenceExecution;

impl<A, AuthContext> ExecuteOcr<DocumentIntelligenceTransformation, A, AuthContext>
    for DocumentIntelligenceExecution
where
    A: Auth,
    AuthContext: Send + Sync,
{
    async fn execute(
        &self,
        client: &OcrClient,
        transformation: &DocumentIntelligenceTransformation,
        request: &LiteLLMOcrRequest,
        params: &<DocumentIntelligenceTransformation as OcrTransformation>::Params,
        endpoint: &str,
        authentication: &ResolvedAuth<A, AuthContext>,
    ) -> Result<LiteLLMOcrResponse, OcrError> {
        let body = transformation.transform_request(OcrTransformRequest {
            model: request.model.clone(),
            document: request.document.clone(),
            params: params.clone(),
        })?;
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
        finish::<DocumentIntelligenceTransformation>(transformation, request, decoded)
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

fn finish<D>(
    transformation: &D,
    request: &LiteLLMOcrRequest,
    decoded: DecodedOcrResponse<D::WireResponse>,
) -> Result<LiteLLMOcrResponse, OcrError>
where
    D: OcrTransformation,
{
    let response = transformation.transform_response(OcrTransformResponse {
        model: request.model.clone(),
        response: decoded.data,
    })?;
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
