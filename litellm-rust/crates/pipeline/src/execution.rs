use std::future::Future;

use litellm_auth::{Auth, ResolvedAuth};
use litellm_operation::{Delivery, OperationCodec};
use litellm_transport::{Authenticated, FinalRequest, Transport};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Error;

pub trait ExecuteOperation<C, A, AuthContext>: Send + Sync
where
    C: OperationCodec,
    A: Auth,
{
    fn execute(
        &self,
        codec: &C,
        call: &C::Call,
        params: &C::Params,
        endpoint: &str,
        auth: &ResolvedAuth<A, AuthContext>,
    ) -> impl Future<Output = Result<C::Response, Error>> + Send;
}

#[derive(Debug)]
pub struct JsonExecution<T> {
    transport: T,
}

impl<T> JsonExecution<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<C, A, AuthContext, T> ExecuteOperation<C, A, AuthContext> for JsonExecution<T>
where
    C: OperationCodec,
    C::WireRequest: Serialize,
    C::WireResponse: DeserializeOwned,
    A: Auth,
    AuthContext: Send + Sync,
    T: Transport + Send + Sync,
{
    async fn execute(
        &self,
        codec: &C,
        call: &C::Call,
        params: &C::Params,
        endpoint: &str,
        auth: &ResolvedAuth<A, AuthContext>,
    ) -> Result<C::Response, Error> {
        let wire_request = codec.encode(call, params, Delivery::Complete)?;
        let body = serde_json::to_vec(&wire_request).map_err(|error| {
            Error::InvalidRequest(format!("request serialization failed: {error}"))
        })?;
        let url = endpoint
            .parse()
            .map_err(|error| Error::InvalidRequest(format!("endpoint url is invalid: {error}")))?;
        let mut request = reqwest::Request::new(reqwest::Method::POST, url);
        *request.body_mut() = Some(body.into());
        request.headers_mut().insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        apply_headers(request.headers_mut(), &codec.protocol_headers())?;
        apply_headers(request.headers_mut(), &auth.headers)?;
        let authenticated: FinalRequest<Authenticated> = FinalRequest::new(request)
            .authenticate(&auth.authenticator)
            .await?;
        let response = self.transport.send(authenticated).await?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(litellm_transport::Error::from)?;
        if !status.is_success() {
            let message = String::from_utf8_lossy(&bytes).chars().take(256).collect();
            return Err(Error::Provider {
                status: status.as_u16(),
                message,
            });
        }
        let wire_response = serde_json::from_slice(&bytes).map_err(|error| {
            Error::InvalidResponse(format!("response deserialization failed: {error}"))
        })?;
        Ok(codec.decode(call, wire_response)?)
    }
}

fn apply_headers(
    headers: &mut reqwest::header::HeaderMap,
    values: &[(String, String)],
) -> Result<(), Error> {
    for (name, value) in values {
        let name = name
            .parse::<reqwest::header::HeaderName>()
            .map_err(|error| Error::InvalidRequest(format!("invalid header name: {error}")))?;
        let value = value
            .parse::<reqwest::header::HeaderValue>()
            .map_err(|error| Error::InvalidRequest(format!("invalid header value: {error}")))?;
        headers.insert(name, value);
    }
    Ok(())
}
