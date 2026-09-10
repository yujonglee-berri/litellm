use crate::auth::Auth;

use super::super::OcrClient;
use super::super::document::InlineDocument;
use super::super::endpoints::complete_reducto_url;
use super::super::error::{OcrError, OcrRequestError, OcrResponseError};
use super::super::prepare::{build_http_request, guardrail_document};
use super::super::transformations::reducto::ReductoUploadResponse;
use super::super::transformations::{OcrTransformRequest, OcrTransformation};
use super::super::types::{LiteLLMOcrRequest, LiteLLMOcrResponse, OcrConnection, OcrDocument};
use super::{ExecuteOcr, finish, send_json};
use crate::auth::ResolvedAuth;

const REDUCTO_ID_PREFIX: &str = "reducto://";

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReductoExecution;

impl<D, A, AuthContext> ExecuteOcr<D, A, AuthContext> for ReductoExecution
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
        let document = guardrail_document(request, endpoint).await?;
        let document = prepare_document(
            client,
            document,
            &request.connection,
            &authentication.headers,
            &authentication.authenticator,
        )
        .await?;
        let body = transformation.transform_request(OcrTransformRequest {
            model: request.model.clone(),
            document,
            params: params.clone(),
        })?;
        let provider_request =
            build_http_request(client, request, endpoint, &authentication.headers, &body)?;
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

async fn prepare_document(
    client: &OcrClient,
    document: OcrDocument,
    connection: &OcrConnection,
    headers: &[(String, String)],
    auth: &impl Auth,
) -> Result<OcrDocument, OcrError> {
    if document.source().starts_with(REDUCTO_ID_PREFIX) {
        if document.source()[REDUCTO_ID_PREFIX.len()..]
            .trim()
            .is_empty()
        {
            return Err(OcrRequestError::RequestField {
                path: "document file id".into(),
            }
            .into());
        }
        return Ok(document);
    }
    let inline = InlineDocument::parse(document.source())?.ok_or(OcrRequestError::ReductoSource)?;
    let mime = inline.mime_type().to_string();
    let bytes = inline.decode(crate::constants::OCR_INLINE_MAX_BYTES)?;
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name("document")
        .mime_str(&mime)
        .map_err(|_| OcrRequestError::InvalidDataUri)?;
    let builder = client
        .provider_http()
        .post(complete_reducto_url(
            connection.api_base.as_deref(),
            "upload",
        )?)
        .multipart(reqwest::multipart::Form::new().part("file", part))
        .timeout(connection.timeout);
    let builder = crate::http_utils::with_headers(
        builder,
        headers,
        crate::http_utils::HeaderPolicy::Except(&["content-type", "content-length"]),
    );
    let request = builder
        .build()
        .map_err(crate::error::TransportError::from)?;
    let request = auth
        .authenticate(request)
        .await
        .map_err(crate::Error::from)?;
    let response = crate::http_utils::http_request(reqwest::RequestBuilder::from_parts(
        client.provider_http().clone(),
        request,
    ))
    .await
    .map_err(crate::error::TransportError::from)?;
    let uploaded =
        super::super::client::read_json_response::<ReductoUploadResponse>(response, false)
            .await?
            .data;
    let file_id = uploaded
        .file_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let Some(file_id) = file_id else {
        return Err(OcrResponseError::ResponseField {
            path: "file_id".into(),
        }
        .into());
    };
    Ok(document.with_source(file_id.to_string()))
}
