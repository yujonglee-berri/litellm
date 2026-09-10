//! AWS credential resolution and SigV4 signing shared by provider operations.

pub mod constants;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_credential_types::Credentials;
use aws_credential_types::provider::ProvideCredentials;
use aws_sigv4::http_request::{
    SignableBody, SignableRequest, SigningParams, SigningSettings, sign,
};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use litellm_auth::error::AwsAuthError;
use litellm_auth::{Auth, AuthError, AuthFuture, AuthScheme};
use reqwest::Request;
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::constants::{
    AWS_ACCESS_KEY_ID, AWS_EXTERNAL_ID, AWS_PROFILE_NAME, AWS_REGION, AWS_REGION_NAME,
    AWS_ROLE_ARN, AWS_ROLE_NAME, AWS_SECRET_ACCESS_KEY, AWS_SESSION_NAME, AWS_SESSION_TOKEN,
    AWS_SIGNED_HEADER_NAMES, AWS_STS_ENDPOINT, AWS_WEB_IDENTITY_TOKEN, AWS_WEB_IDENTITY_TOKEN_FILE,
    BEDROCK_SERVICE, DEFAULT_BEDROCK_REGION, DEFAULT_SESSION_NAME_PREFIX,
    SIGV4_COMPUTED_HEADER_NAMES,
};

const STATIC_CREDENTIALS_TTL: Duration = Duration::from_secs(3600 - 60);
const AMBIENT_CREDENTIALS_TTL: Duration = Duration::from_secs(600);

static IAM_CREDENTIALS_CACHE: OnceLock<
    Mutex<std::collections::HashMap<String, CachedCredentials>>,
> = OnceLock::new();

#[derive(Clone)]
struct CachedCredentials {
    credentials: Credentials,
    expires_at: SystemTime,
}

fn credential_cache_ttl(flow: &AwsAuthFlow) -> Option<Duration> {
    match flow {
        AwsAuthFlow::StaticKeys { .. } => Some(STATIC_CREDENTIALS_TTL),
        AwsAuthFlow::DefaultChain => Some(AMBIENT_CREDENTIALS_TTL),
        AwsAuthFlow::WebIdentity { .. }
        | AwsAuthFlow::AssumeRole { .. }
        | AwsAuthFlow::Profile { .. }
        | AwsAuthFlow::SessionToken { .. } => None,
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct AwsAuthConfig {
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
    pub region_name: Option<String>,
    pub session_name: Option<String>,
    pub profile_name: Option<String>,
    pub role_name: Option<String>,
    pub web_identity_token: Option<String>,
    pub sts_endpoint: Option<String>,
    pub external_id: Option<String>,
}

impl fmt::Debug for AwsAuthConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AwsAuthConfig")
            .field("access_key_id", &redacted(&self.access_key_id))
            .field("secret_access_key", &redacted(&self.secret_access_key))
            .field("session_token", &redacted(&self.session_token))
            .field("region_name", &self.region_name)
            .field("session_name", &self.session_name)
            .field("profile_name", &self.profile_name)
            .field("role_name", &self.role_name)
            .field("web_identity_token", &redacted(&self.web_identity_token))
            .field("sts_endpoint", &self.sts_endpoint)
            .field("external_id", &redacted(&self.external_id))
            .finish()
    }
}

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

impl AwsAuthConfig {
    fn with_environment(self, env_lookup: &(dyn Fn(&str) -> Option<String> + Sync)) -> Self {
        Self {
            access_key_id: self.access_key_id.or_else(|| env_lookup(AWS_ACCESS_KEY_ID)),
            secret_access_key: self
                .secret_access_key
                .or_else(|| env_lookup(AWS_SECRET_ACCESS_KEY)),
            session_token: self.session_token.or_else(|| env_lookup(AWS_SESSION_TOKEN)),
            region_name: self.region_name.or_else(|| env_lookup(AWS_REGION_NAME)),
            session_name: self.session_name.or_else(|| env_lookup(AWS_SESSION_NAME)),
            profile_name: self.profile_name.or_else(|| env_lookup(AWS_PROFILE_NAME)),
            role_name: self.role_name.or_else(|| env_lookup(AWS_ROLE_NAME)),
            web_identity_token: self
                .web_identity_token
                .or_else(|| env_lookup(AWS_WEB_IDENTITY_TOKEN)),
            sts_endpoint: self.sts_endpoint.or_else(|| env_lookup(AWS_STS_ENDPOINT)),
            external_id: self.external_id.or_else(|| env_lookup(AWS_EXTERNAL_ID)),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum AwsAuthFlow {
    WebIdentity {
        token: String,
        role: String,
        session_name: String,
    },
    AssumeRole {
        role: String,
        session_name: Option<String>,
    },
    Profile {
        name: String,
    },
    SessionToken {
        access_key_id: String,
        secret_access_key: String,
        session_token: String,
    },
    StaticKeys {
        access_key_id: String,
        secret_access_key: String,
        region_name: String,
    },
    DefaultChain,
}

impl fmt::Debug for AwsAuthFlow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WebIdentity {
                role, session_name, ..
            } => formatter
                .debug_struct("WebIdentity")
                .field("token", &"[REDACTED]")
                .field("role", role)
                .field("session_name", session_name)
                .finish(),
            Self::AssumeRole { role, session_name } => formatter
                .debug_struct("AssumeRole")
                .field("role", role)
                .field("session_name", session_name)
                .finish(),
            Self::Profile { name } => formatter
                .debug_struct("Profile")
                .field("name", name)
                .finish(),
            Self::SessionToken { .. } => formatter
                .debug_struct("SessionToken")
                .field("access_key_id", &"[REDACTED]")
                .field("secret_access_key", &"[REDACTED]")
                .field("session_token", &"[REDACTED]")
                .finish(),
            Self::StaticKeys { region_name, .. } => formatter
                .debug_struct("StaticKeys")
                .field("access_key_id", &"[REDACTED]")
                .field("secret_access_key", &"[REDACTED]")
                .field("region_name", region_name)
                .finish(),
            Self::DefaultChain => formatter.write_str("DefaultChain"),
        }
    }
}

fn redacted(value: &Option<String>) -> Option<&'static str> {
    value.as_ref().map(|_| "[REDACTED]")
}

fn cache_key(config: &AwsAuthConfig, flow: &AwsAuthFlow) -> String {
    let mut hasher = Sha256::new();
    hash_config(&mut hasher, config);
    hash_flow(&mut hasher, flow);
    format!("{:x}", hasher.finalize())
}

fn hash_config(hasher: &mut Sha256, config: &AwsAuthConfig) {
    for value in [
        config.access_key_id.as_deref(),
        config.secret_access_key.as_deref(),
        config.session_token.as_deref(),
        config.region_name.as_deref(),
        config.session_name.as_deref(),
        config.profile_name.as_deref(),
        config.role_name.as_deref(),
        config.web_identity_token.as_deref(),
        config.sts_endpoint.as_deref(),
        config.external_id.as_deref(),
    ] {
        hash_optional(hasher, value);
    }
}

fn hash_flow(hasher: &mut Sha256, flow: &AwsAuthFlow) {
    match flow {
        AwsAuthFlow::WebIdentity {
            token,
            role,
            session_name,
        } => hash_values(hasher, "web_identity", [token, role, session_name]),
        AwsAuthFlow::AssumeRole { role, session_name } => {
            hash_values(hasher, "assume_role", [Some(role), session_name.as_ref()])
        }
        AwsAuthFlow::Profile { name } => hash_values(hasher, "profile", [name]),
        AwsAuthFlow::SessionToken {
            access_key_id,
            secret_access_key,
            session_token,
        } => hash_values(
            hasher,
            "session_token",
            [access_key_id, secret_access_key, session_token],
        ),
        AwsAuthFlow::StaticKeys {
            access_key_id,
            secret_access_key,
            region_name,
        } => hash_values(
            hasher,
            "static_keys",
            [access_key_id, secret_access_key, region_name],
        ),
        AwsAuthFlow::DefaultChain => hash_values(hasher, "default_chain", [] as [&String; 0]),
    }
}

fn hash_values<'a>(
    hasher: &mut Sha256,
    kind: &str,
    values: impl IntoIterator<Item = impl Into<Option<&'a String>>>,
) {
    hasher.update(kind.as_bytes());
    for value in values {
        hash_optional(hasher, value.into().map(String::as_str));
    }
}

fn hash_optional(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.as_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.update([0xff]);
}

fn get_cached_credentials(key: &str) -> Option<Credentials> {
    let cache = IAM_CREDENTIALS_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut entries = cache.lock().ok()?;
    let entry = entries.get(key)?;
    if entry.expires_at > SystemTime::now() {
        return Some(entry.credentials.clone());
    }
    entries.remove(key);
    None
}

fn set_cached_credentials(key: String, credentials: Credentials, ttl: Duration) {
    let cache = IAM_CREDENTIALS_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(mut entries) = cache.lock()
        && let Some(expires_at) = SystemTime::now().checked_add(ttl)
    {
        entries.insert(
            key,
            CachedCredentials {
                credentials,
                expires_at,
            },
        );
    }
}

fn role_identity(arn: &str) -> Option<(&str, &str, &str)> {
    let mut parts = arn.splitn(6, ':');
    let ("arn", partition, _, _, account, resource) = (
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
    ) else {
        return None;
    };
    let role = if let Some(role) = resource.strip_prefix("role/") {
        role.rsplit('/').next()?
    } else {
        resource.strip_prefix("assumed-role/")?.split('/').next()?
    };
    Some((partition, account, role))
}

fn same_role_arns(target: &str, caller: &str) -> bool {
    role_identity(target) == role_identity(caller)
}

pub fn classify_auth(
    config: AwsAuthConfig,
    env_lookup: &(dyn Fn(&str) -> Option<String> + Sync),
) -> AwsAuthFlow {
    let config = config.with_environment(env_lookup);
    if let (Some(token), Some(role), Some(session_name)) = (
        config.web_identity_token.clone(),
        config.role_name.clone(),
        config.session_name.clone(),
    ) {
        return AwsAuthFlow::WebIdentity {
            token,
            role,
            session_name,
        };
    }
    if let Some(role) = config.role_name.clone() {
        return AwsAuthFlow::AssumeRole {
            role,
            session_name: config.session_name.clone(),
        };
    }
    if let Some(name) = config.profile_name {
        return AwsAuthFlow::Profile { name };
    }
    if let (Some(access_key_id), Some(secret_access_key), Some(session_token)) = (
        config.access_key_id.clone(),
        config.secret_access_key.clone(),
        config.session_token,
    ) {
        return AwsAuthFlow::SessionToken {
            access_key_id,
            secret_access_key,
            session_token,
        };
    }
    if let (Some(access_key_id), Some(secret_access_key), Some(region_name)) = (
        config.access_key_id,
        config.secret_access_key,
        config.region_name,
    ) {
        return AwsAuthFlow::StaticKeys {
            access_key_id,
            secret_access_key,
            region_name,
        };
    }
    AwsAuthFlow::DefaultChain
}

pub async fn resolve_credentials(
    config: AwsAuthConfig,
    env_lookup: &(dyn Fn(&str) -> Option<String> + Sync),
) -> Result<Credentials, AuthError> {
    let resolved = config.clone().with_environment(env_lookup);
    let flow = classify_auth(config, env_lookup);
    match flow {
        AwsAuthFlow::SessionToken {
            access_key_id,
            secret_access_key,
            session_token,
        } => Ok(Credentials::new(
            access_key_id,
            secret_access_key,
            Some(session_token),
            None,
            "litellm-static-session",
        )),
        AwsAuthFlow::StaticKeys {
            access_key_id,
            secret_access_key,
            region_name,
        } => {
            let flow = AwsAuthFlow::StaticKeys {
                access_key_id: access_key_id.clone(),
                secret_access_key: secret_access_key.clone(),
                region_name,
            };
            let key = cache_key(&resolved, &flow);
            if let Some(credentials) = get_cached_credentials(&key) {
                return Ok(credentials);
            }
            let credentials = Credentials::new(
                access_key_id,
                secret_access_key,
                None,
                None,
                "litellm-static",
            );
            set_cached_credentials(
                key,
                credentials.clone(),
                credential_cache_ttl(&flow).unwrap_or(STATIC_CREDENTIALS_TTL),
            );
            Ok(credentials)
        }
        AwsAuthFlow::Profile { name } => {
            let provider = aws_config::profile::ProfileFileCredentialsProvider::builder()
                .profile_name(name)
                .build();
            provider
                .provide_credentials()
                .await
                .map_err(|error| AwsAuthError::Profile(error.to_string()).into())
        }
        AwsAuthFlow::AssumeRole { role, session_name } => {
            if is_already_running_as_role(&role, &resolved).await? {
                let ambient_flow = AwsAuthFlow::DefaultChain;
                let key = cache_key(&resolved, &ambient_flow);
                if let Some(credentials) = get_cached_credentials(&key) {
                    return Ok(credentials);
                }
                let provider =
                    aws_config::default_provider::credentials::DefaultCredentialsChain::builder()
                        .build()
                        .await;
                let credentials = provider.provide_credentials().await.map_err(|error| {
                    AuthError::from(AwsAuthError::DefaultChain(error.to_string()))
                })?;
                set_cached_credentials(
                    key,
                    credentials.clone(),
                    credential_cache_ttl(&ambient_flow).unwrap_or(AMBIENT_CREDENTIALS_TTL),
                );
                return Ok(credentials);
            }
            let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
            if let Some(region) = resolved.region_name.clone() {
                loader = loader.region(aws_types::region::Region::new(region));
            }
            if let Some(endpoint) = resolved.sts_endpoint.clone() {
                loader = loader.endpoint_url(endpoint);
            }
            if let (Some(access_key_id), Some(secret_access_key)) =
                (resolved.access_key_id, resolved.secret_access_key)
            {
                loader = loader.credentials_provider(Credentials::new(
                    access_key_id,
                    secret_access_key,
                    resolved.session_token,
                    None,
                    "litellm-role-source",
                ));
            }
            let sdk_config = loader.load().await;
            let builder = aws_config::sts::AssumeRoleProvider::builder(role);
            let builder = match session_name {
                Some(name) => builder.session_name(name),
                None => builder.session_name(default_session_name()),
            };
            let builder = match resolved.external_id {
                Some(id) => builder.external_id(id),
                None => builder,
            };
            let provider = builder.configure(&sdk_config).build().await;
            provider
                .provide_credentials()
                .await
                .map_err(|error| AwsAuthError::AssumeRole(error.to_string()).into())
        }
        AwsAuthFlow::WebIdentity {
            token,
            role,
            session_name,
        } => {
            let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
            if let Some(region) = resolved.region_name {
                loader = loader.region(aws_types::region::Region::new(region));
            }
            if let Some(endpoint) = resolved.sts_endpoint {
                loader = loader.endpoint_url(endpoint);
            }
            let sdk_config = loader.load().await;
            let client = aws_sdk_sts::Client::new(&sdk_config);
            let response = client
                .assume_role_with_web_identity()
                .role_arn(role)
                .role_session_name(session_name)
                .web_identity_token(token)
                .send()
                .await
                .map_err(|error| AuthError::from(AwsAuthError::WebIdentity(error.to_string())))?;
            let credentials = response
                .credentials()
                .ok_or_else(|| AuthError::from(AwsAuthError::MissingWebIdentityCredentials))?;
            let expiration = SystemTime::try_from(*credentials.expiration()).map_err(|error| {
                AuthError::from(AwsAuthError::WebIdentityExpiration(error.to_string()))
            })?;
            Ok(Credentials::new(
                credentials.access_key_id(),
                credentials.secret_access_key(),
                Some(credentials.session_token().to_string()),
                Some(expiration),
                "litellm-web-identity",
            ))
        }
        AwsAuthFlow::DefaultChain => {
            let key = cache_key(&resolved, &AwsAuthFlow::DefaultChain);
            if let Some(credentials) = get_cached_credentials(&key) {
                return Ok(credentials);
            }
            let provider =
                aws_config::default_provider::credentials::DefaultCredentialsChain::builder()
                    .build()
                    .await;
            let credentials = provider
                .provide_credentials()
                .await
                .map_err(|error| AwsAuthError::DefaultChain(error.to_string()))?;
            set_cached_credentials(
                key,
                credentials.clone(),
                credential_cache_ttl(&AwsAuthFlow::DefaultChain).unwrap_or(AMBIENT_CREDENTIALS_TTL),
            );
            Ok(credentials)
        }
    }
}

async fn is_already_running_as_role(role: &str, config: &AwsAuthConfig) -> Result<bool, AuthError> {
    if role_identity(role).is_none() {
        return Ok(false);
    }
    if let (Ok(current_role), Ok(token_file)) = (
        std::env::var(AWS_ROLE_ARN),
        std::env::var(AWS_WEB_IDENTITY_TOKEN_FILE),
    ) && !token_file.is_empty()
    {
        return Ok(same_role_arns(role, &current_role));
    }

    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(region) = config.region_name.clone() {
        loader = loader.region(aws_types::region::Region::new(region));
    }
    if let Some(endpoint) = config.sts_endpoint.clone() {
        loader = loader.endpoint_url(endpoint);
    }
    let sdk_config = loader.load().await;
    let response = match aws_sdk_sts::Client::new(&sdk_config)
        .get_caller_identity()
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(false),
    };
    Ok(response
        .arn()
        .is_some_and(|caller| same_role_arns(role, caller)))
}

fn default_session_name() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    format!("{DEFAULT_SESSION_NAME_PREFIX}-{seconds}")
}

/// The subset of `headers` SigV4 should cover.
///
/// Python signs only these and reattaches the rest afterwards, so a forwarded
/// client header cannot change the canonical request and invalidate the
/// signature. Signing everything instead makes the request 403 on a header the
/// caller supplied, on a deployment that works on the Python path.
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

/// Whether the signer produces `name` itself.
///
/// Python's reattach loop skips these, so a caller-supplied copy never reaches
/// the wire next to the computed one.
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

/// Model-id and region parsing shared by every Bedrock route.
pub fn bedrock_model_id_and_region(model: &str) -> (String, Option<String>) {
    let mut stripped = model;
    for prefix in ["bedrock/converse/", "bedrock/", "converse/"] {
        if let Some(value) = stripped.strip_prefix(prefix) {
            stripped = value;
            break;
        }
    }
    let mut region = None;
    if let Some((candidate, remainder)) = stripped.split_once('/')
        && is_bedrock_region(candidate)
    {
        region = Some(candidate.to_string());
        stripped = remainder;
    }
    for prefix in ["nova-2/", "nova/"] {
        if let Some(value) = stripped.strip_prefix(prefix) {
            stripped = value;
            break;
        }
    }
    if region.is_none() {
        // Python splits the whole ARN and takes field 3, the region. Stripping
        // `arn:` first shifts every field down one, so the region is field 2
        // here; field 3 is the account id.
        region = stripped
            .strip_prefix("arn:")
            .and_then(|value| value.split(':').nth(2))
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }
    (stripped.to_string(), region)
}

fn is_bedrock_region(value: &str) -> bool {
    value.len() > 3
        && value.contains('-')
        && value
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || char == '-')
}

pub fn resolve_bedrock_region(
    model_region: Option<&str>,
    optional_params: &Map<String, Value>,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> String {
    if let Some(region) = optional_params
        .get("aws_region_name")
        .and_then(Value::as_str)
    {
        return region.to_string();
    }
    if let Some(region) = model_region {
        return region.to_string();
    }
    env_lookup(AWS_REGION_NAME)
        .or_else(|| env_lookup(AWS_REGION))
        .unwrap_or_else(|| DEFAULT_BEDROCK_REGION.to_string())
}

pub fn aws_auth_config(
    optional_params: &Map<String, Value>,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> AwsAuthConfig {
    let value = |key: &str| {
        optional_params
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let env = |key: &str| env_lookup(key);
    AwsAuthConfig {
        access_key_id: value("aws_access_key_id").or_else(|| env("AWS_ACCESS_KEY_ID")),
        secret_access_key: value("aws_secret_access_key").or_else(|| env("AWS_SECRET_ACCESS_KEY")),
        session_token: value("aws_session_token").or_else(|| env("AWS_SESSION_TOKEN")),
        region_name: value("aws_region_name").or_else(|| env(AWS_REGION_NAME)),
        session_name: value("aws_session_name").or_else(|| env("AWS_SESSION_NAME")),
        profile_name: value("aws_profile_name").or_else(|| env("AWS_PROFILE_NAME")),
        role_name: value("aws_role_name").or_else(|| env("AWS_ROLE_NAME")),
        web_identity_token: value("aws_web_identity_token")
            .or_else(|| env("AWS_WEB_IDENTITY_TOKEN")),
        sts_endpoint: value("aws_sts_endpoint").or_else(|| env("AWS_STS_ENDPOINT")),
        external_id: value("aws_external_id").or_else(|| env("AWS_EXTERNAL_ID")),
    }
}

/// Credentials a host resolved through its own chain and handed down verbatim.
///
/// A host with its own resolution (LiteLLM's Python `BaseAWSLLM`, which reads
/// profiles, STS and boto sessions) passes the result here so the core signs
/// with exactly those. Without this the core would re-derive from ambient
/// state, where an unrelated `AWS_ROLE_NAME` or `AWS_PROFILE_NAME` in the
/// environment outranks explicit keys in [`classify_auth`] and the two sides
/// would sign as different principals.
pub fn host_supplied_credentials(optional_params: &Map<String, Value>) -> Option<Credentials> {
    let value = |key: &str| {
        optional_params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let access_key_id = value("aws_access_key_id")?;
    let secret_access_key = value("aws_secret_access_key")?;
    Some(Credentials::new(
        access_key_id,
        secret_access_key,
        value("aws_session_token").map(str::to_string),
        None,
        "litellm-host-supplied",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn parity_inputs() -> (String, Vec<u8>, BTreeMap<String, String>) {
        (
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/amazon.titan-text-express-v1/invoke"
                .to_string(),
            br#"{"input":"hello"}"#.to_vec(),
            BTreeMap::from([("Content-Type".to_string(), "application/json".to_string())]),
        )
    }

    #[test]
    fn reads_the_region_field_of_a_model_arn_not_the_account_id() {
        // Python's `_get_aws_region_from_model_arn` splits the whole ARN and
        // takes field 3. Stripping `arn:` first shifts every field down one, so
        // the region is field 2 here. Taking field 3 after the strip returns
        // the account id, which is not a region at all.
        let (_, region) = bedrock_model_id_and_region(
            "bedrock/arn:aws:bedrock:us-west-2:123456789012:foundation-model/anthropic.claude-v2",
        );
        assert_eq!(region.as_deref(), Some("us-west-2"));
    }

    #[test]
    fn classification_preserves_python_precedence() {
        let config = AwsAuthConfig {
            access_key_id: Some("ak".into()),
            secret_access_key: Some("sk".into()),
            session_token: Some("token".into()),
            region_name: Some("us-east-1".into()),
            session_name: Some("session".into()),
            profile_name: Some("profile".into()),
            role_name: Some("role".into()),
            web_identity_token: Some("oidc".into()),
            ..Default::default()
        };
        assert!(matches!(
            classify_auth(config, &no_env),
            AwsAuthFlow::WebIdentity { .. }
        ));
    }

    #[test]
    fn debug_redacts_every_credential_value() {
        let config = AwsAuthConfig {
            access_key_id: Some("access-id".into()),
            secret_access_key: Some("secret-key".into()),
            session_token: Some("session-token".into()),
            web_identity_token: Some("web-token".into()),
            external_id: Some("external-id".into()),
            ..Default::default()
        };
        let flow = classify_auth(config.clone(), &no_env);
        let debug = format!("{config:?} {flow:?}");

        for secret in [
            "access-id",
            "secret-key",
            "session-token",
            "web-token",
            "external-id",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn cache_identity_still_includes_redacted_credentials() {
        let first = AwsAuthConfig {
            access_key_id: Some("first".into()),
            secret_access_key: Some("secret".into()),
            region_name: Some("us-east-1".into()),
            ..Default::default()
        };
        let second = AwsAuthConfig {
            access_key_id: Some("second".into()),
            ..first.clone()
        };

        let first_flow = classify_auth(first.clone(), &no_env);
        let second_flow = classify_auth(second.clone(), &no_env);
        assert_ne!(
            cache_key(&first, &first_flow),
            cache_key(&second, &second_flow)
        );
    }

    #[test]
    fn classification_covers_fallthroughs() {
        let env = |key: &str| match key {
            AWS_PROFILE_NAME => Some("profile".into()),
            _ => None,
        };
        assert!(matches!(
            classify_auth(AwsAuthConfig::default(), &env),
            AwsAuthFlow::Profile { .. }
        ));
        assert!(matches!(
            classify_auth(
                AwsAuthConfig {
                    access_key_id: Some("ak".into()),
                    secret_access_key: Some("sk".into()),
                    session_token: Some("token".into()),
                    ..Default::default()
                },
                &no_env
            ),
            AwsAuthFlow::SessionToken { .. }
        ));
        assert!(matches!(
            classify_auth(
                AwsAuthConfig {
                    access_key_id: Some("ak".into()),
                    secret_access_key: Some("sk".into()),
                    region_name: Some("us-east-1".into()),
                    ..Default::default()
                },
                &no_env
            ),
            AwsAuthFlow::StaticKeys { .. }
        ));
        assert_eq!(
            classify_auth(AwsAuthConfig::default(), &no_env),
            AwsAuthFlow::DefaultChain
        );
    }

    #[tokio::test]
    async fn static_credentials_do_not_use_network() {
        let credentials = resolve_credentials(
            AwsAuthConfig {
                access_key_id: Some("ak".into()),
                secret_access_key: Some("sk".into()),
                region_name: Some("us-east-1".into()),
                ..Default::default()
            },
            &no_env,
        )
        .await
        .expect("static credentials");
        assert_eq!(credentials.access_key_id(), "ak");
        assert_eq!(credentials.session_token(), None);
    }

    #[test]
    fn cache_policy_matches_python_flows() {
        assert_eq!(
            credential_cache_ttl(&AwsAuthFlow::StaticKeys {
                access_key_id: "ak".into(),
                secret_access_key: "sk".into(),
                region_name: "us-east-1".into(),
            }),
            Some(STATIC_CREDENTIALS_TTL)
        );
        assert_eq!(
            credential_cache_ttl(&AwsAuthFlow::DefaultChain),
            Some(AMBIENT_CREDENTIALS_TTL)
        );
        assert_eq!(
            credential_cache_ttl(&AwsAuthFlow::SessionToken {
                access_key_id: "ak".into(),
                secret_access_key: "sk".into(),
                session_token: "token".into(),
            }),
            None
        );
        assert_eq!(
            credential_cache_ttl(&AwsAuthFlow::Profile {
                name: "profile".into()
            }),
            None
        );
        assert_eq!(
            credential_cache_ttl(&AwsAuthFlow::AssumeRole {
                role: "arn:aws:iam::123456789012:role/demo".into(),
                session_name: None,
            }),
            None
        );
        assert_eq!(
            credential_cache_ttl(&AwsAuthFlow::WebIdentity {
                token: "token".into(),
                role: "arn:aws:iam::123456789012:role/demo".into(),
                session_name: "session".into(),
            }),
            None
        );
    }

    #[test]
    fn cache_round_trip_preserves_credentials() {
        let key = format!("cache-test-{}", std::process::id());
        let credentials = Credentials::new("cache-ak", "cache-sk", None, None, "test");
        set_cached_credentials(key.clone(), credentials.clone(), STATIC_CREDENTIALS_TTL);
        assert_eq!(
            get_cached_credentials(&key).map(|value| value.access_key_id().to_string()),
            Some("cache-ak".to_string())
        );
    }

    #[test]
    fn same_role_comparison_matches_partition_account_and_role() {
        assert!(same_role_arns(
            "arn:aws:iam::123456789012:role/path/demo",
            "arn:aws:sts::123456789012:assumed-role/demo/session"
        ));
        assert!(!same_role_arns(
            "arn:aws:iam::123456789012:role/demo",
            "arn:aws:iam::999999999999:role/demo"
        ));
        assert!(!same_role_arns(
            "arn:aws:iam::123456789012:role/demo",
            "arn:aws-cn:iam::123456789012:role/demo"
        ));
        assert!(!same_role_arns(
            "arn:aws:iam::123456789012:user/demo",
            "arn:aws:iam::123456789012:role/demo"
        ));
    }

    #[test]
    fn a_forwarded_client_header_is_not_folded_into_the_signature() {
        // Python signs only the AWS header set, so a header a caller forwarded
        // cannot change the canonical request. Signing it instead makes the
        // request 403 the moment anything on the wire rewrites or drops it.
        let (url, body, mut headers) = parity_inputs();
        headers.insert("x-request-id".to_string(), "abc-123".to_string());
        headers.insert("Accept-Encoding".to_string(), "gzip".to_string());
        headers.insert("x-amzn-trace-id".to_string(), "Root=1-abc".to_string());
        let signable = aws_signature_headers(&headers);

        assert!(!signable.contains_key("x-request-id"));
        assert!(!signable.contains_key("Accept-Encoding"));
        // The AWS-prefixed one is genuinely part of the signature.
        assert!(signable.contains_key("x-amzn-trace-id"));
        assert!(signable.contains_key("Content-Type"));

        let credentials = Credentials::new(
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            None,
            None,
            "test",
        );
        let signed = sign_bedrock_post(
            &url,
            &body,
            &signable,
            "us-east-1",
            &credentials,
            SystemTime::UNIX_EPOCH,
        )
        .expect("signs");
        let authorization = signed
            .get("Authorization")
            .expect("carries an authorization header");
        assert!(
            !authorization.contains("x-request-id"),
            "forwarded header reached SignedHeaders: {authorization}"
        );
        assert!(
            !authorization.contains("accept-encoding"),
            "forwarded header reached SignedHeaders: {authorization}"
        );
    }

    #[test]
    fn signing_matches_botocore_golden_vector() {
        let (url, body, headers) = parity_inputs();
        let credentials = Credentials::new(
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            Some("session-token".to_string()),
            None,
            "test",
        );
        let signed = sign_bedrock_post(
            &url,
            &body,
            &headers,
            "us-east-1",
            &credentials,
            UNIX_EPOCH + std::time::Duration::from_secs(1_704_164_645),
        )
        .expect("golden signature");
        assert_eq!(
            signed.get("X-Amz-Date").map(String::as_str),
            Some("20240102T030405Z")
        );
        assert_eq!(
            signed.get("X-Amz-Security-Token").map(String::as_str),
            Some("session-token")
        );
        assert_eq!(
            signed.get("Authorization").map(String::as_str),
            Some(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20240102/us-east-1/bedrock/aws4_request, SignedHeaders=content-type;host;x-amz-date;x-amz-security-token, Signature=55c027ef47527d3ad63f1735f9d099efdbc99f296ff914bd94e727e24ec0e464"
            )
        );
    }

    #[test]
    fn signing_without_session_token_omits_security_header() {
        let (url, body, headers) = parity_inputs();
        let credentials = Credentials::new(
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            None,
            None,
            "test",
        );
        let signed = sign_bedrock_post(
            &url,
            &body,
            &headers,
            "us-east-1",
            &credentials,
            UNIX_EPOCH + std::time::Duration::from_secs(1_704_164_645),
        )
        .expect("signature");
        assert!(!signed.contains_key("X-Amz-Security-Token"));
    }

    #[ignore]
    #[tokio::test]
    async fn live_bedrock_invoke_model_returns_200() -> Result<(), Box<dyn std::error::Error>> {
        let access_key_id = std::env::var("AWS_BEDROCK_TEST_ACCESS_KEY_ID")?;
        let secret_access_key = std::env::var("AWS_BEDROCK_TEST_SECRET_ACCESS_KEY")?;
        let body = br#"{"anthropic_version":"bedrock-2023-05-31","max_tokens":1,"messages":[{"role":"user","content":[{"type":"text","text":"ping"}]}]}"#.to_vec();
        let headers =
            BTreeMap::from([("Content-Type".to_string(), "application/json".to_string())]);
        let credentials = resolve_credentials(
            AwsAuthConfig {
                access_key_id: Some(access_key_id),
                secret_access_key: Some(secret_access_key),
                region_name: Some("us-west-2".to_string()),
                ..Default::default()
            },
            &no_env,
        )
        .await?;
        let client = reqwest::Client::new();
        let mut failures = Vec::new();

        for region in ["us-west-2", "us-east-1"] {
            let url = format!(
                "https://bedrock-runtime.{region}.amazonaws.com/model/us.anthropic.claude-opus-4-8/invoke"
            );
            let signed_headers = sign_bedrock_post(
                &url,
                &body,
                &headers,
                region,
                &credentials,
                SystemTime::now(),
            )?;
            let mut request = client.post(&url).body(body.clone());
            for (name, value) in &headers {
                request = request.header(name, value);
            }
            for (name, value) in signed_headers {
                request = request.header(name, value);
            }
            let response = request.send().await?;
            let status = response.status();
            let response_body = response.text().await?;
            let snippet: String = response_body.chars().take(240).collect();
            println!("region={region} status={status} response={snippet}");
            if status == reqwest::StatusCode::OK {
                return Ok(());
            }
            failures.push(format!("{region}: {status} {snippet}"));
        }

        panic!(
            "no Bedrock region returned HTTP 200: {}",
            failures.join("; ")
        );
    }
}
