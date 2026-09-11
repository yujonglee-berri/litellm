use std::collections::BTreeMap;
use std::time::SystemTime;

use aws_credential_types::Credentials;
use aws_sigv4::http_request::{
    SignableBody, SignableRequest, SigningParams, SigningSettings, sign,
};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use litellm_auth::error::AwsAuthError;
use litellm_auth::{Auth, AuthError, AuthFuture, AuthScheme};
use reqwest::Request;
use reqwest::header::{HeaderName, HeaderValue};

use crate::config::AwsAuthConfig;
use crate::constants::{AWS_SIGNED_HEADER_NAMES, BEDROCK_SERVICE, SIGV4_COMPUTED_HEADER_NAMES};
use crate::credentials::resolve_credentials;

#[derive(Clone, Debug)]
pub struct AwsSigV4Auth {
    service: &'static str,
    region: String,
    config: AwsAuthConfig,
    credentials: Option<Credentials>,
}

pub struct AwsSignableRequest<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub body: &'a [u8],
    pub headers: &'a BTreeMap<String, String>,
    pub region: &'a str,
    pub service: &'a str,
    pub signing_time: SystemTime,
}

impl AwsSigV4Auth {
    pub fn new(
        service: &'static str,
        region: impl Into<String>,
        config: AwsAuthConfig,
        credentials: Option<Credentials>,
    ) -> Self {
        Self {
            service,
            region: region.into(),
            config,
            credentials,
        }
    }
}

impl Auth for AwsSigV4Auth {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::AwsSigV4
    }

    fn authenticate(&self, mut request: Request) -> AuthFuture<'_> {
        Box::pin(async move {
            let unsigned = request_headers(&request)?;
            if let Some(name) = unsigned.keys().find(|name| is_sigv4_computed_header(name)) {
                return Err(AwsAuthError::ComputedHeader(name.clone()).into());
            }
            let body = request
                .body()
                .and_then(reqwest::Body::as_bytes)
                .ok_or(AwsAuthError::StreamingBody)?;
            let env_lookup = |key: &str| std::env::var(key).ok();
            let credentials = match &self.credentials {
                Some(credentials) => credentials.clone(),
                None => resolve_credentials(self.config.clone(), &env_lookup).await?,
            };
            let headers = aws_signature_headers(&unsigned);
            let signature = sign_request(
                AwsSignableRequest {
                    method: request.method().as_str(),
                    url: request.url().as_str(),
                    body,
                    headers: &headers,
                    region: &self.region,
                    service: self.service,
                    signing_time: SystemTime::now(),
                },
                &credentials,
            )?;
            for (name, value) in signature {
                let name = HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| AwsAuthError::InvalidSignedHeader)?;
                let mut value =
                    HeaderValue::from_str(&value).map_err(|_| AwsAuthError::InvalidSignedHeader)?;
                if name == reqwest::header::AUTHORIZATION
                    || name.as_str().eq_ignore_ascii_case("x-amz-security-token")
                {
                    value.set_sensitive(true);
                }
                request.headers_mut().insert(name, value);
            }
            Ok(request)
        })
    }
}

fn request_headers(request: &Request) -> Result<BTreeMap<String, String>, AuthError> {
    request
        .headers()
        .iter()
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.as_str().to_string(), value.to_string()))
                .map_err(|_| AuthError::from(AwsAuthError::InvalidSignedHeader))
        })
        .collect()
}

pub fn aws_signature_headers(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter(|(name, _)| {
            let name = name.to_ascii_lowercase();
            AWS_SIGNED_HEADER_NAMES.contains(&name.as_str())
                || name.starts_with("x-amz-")
                || name.starts_with("x-amzn-")
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

pub fn is_sigv4_computed_header(name: &str) -> bool {
    SIGV4_COMPUTED_HEADER_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

pub fn sign_bedrock_post(
    url: &str,
    body: &[u8],
    headers: &BTreeMap<String, String>,
    region: &str,
    credentials: &Credentials,
    signing_time: SystemTime,
) -> Result<BTreeMap<String, String>, AuthError> {
    sign_request(
        AwsSignableRequest {
            method: "POST",
            url,
            body,
            headers,
            region,
            service: BEDROCK_SERVICE,
            signing_time,
        },
        credentials,
    )
}

pub fn sign_request(
    request: AwsSignableRequest<'_>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, String>, AuthError> {
    let identity: Identity = credentials.clone().into();
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region(request.region)
        .name(request.service)
        .time(request.signing_time)
        .settings(SigningSettings::default())
        .build()
        .map(SigningParams::from)
        .map_err(|error| AwsAuthError::SigningParameters(error.to_string()))?;
    let header_refs = request
        .headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()));
    let request = SignableRequest::new(
        request.method,
        request.url,
        header_refs,
        SignableBody::Bytes(request.body),
    )
    .map_err(|error| AwsAuthError::SignableRequest(error.to_string()))?;
    let (instructions, _) = sign(request, &params)
        .map_err(|error| AwsAuthError::Signing(error.to_string()))?
        .into_parts();
    Ok(instructions
        .headers()
        .map(|(name, value)| {
            let normalized_name = match name {
                "authorization" => "Authorization",
                "x-amz-date" => "X-Amz-Date",
                "x-amz-security-token" => "X-Amz-Security-Token",
                _ => name,
            };
            (normalized_name.to_string(), value.to_string())
        })
        .collect())
}
