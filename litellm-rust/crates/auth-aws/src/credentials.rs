use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aws_credential_types::Credentials;
use aws_credential_types::provider::ProvideCredentials;
use litellm_auth::AuthError;
use litellm_auth::error::AwsAuthError;
use sha2::{Digest, Sha256};

use crate::config::{AwsAuthConfig, AwsAuthFlow, classify_auth};
use crate::constants::{AWS_ROLE_ARN, AWS_WEB_IDENTITY_TOKEN_FILE, DEFAULT_SESSION_NAME_PREFIX};

pub(crate) const STATIC_CREDENTIALS_TTL: Duration = Duration::from_secs(3600 - 60);
pub(crate) const AMBIENT_CREDENTIALS_TTL: Duration = Duration::from_secs(600);
pub(crate) const CREDENTIAL_EXPIRY_SAFETY_WINDOW: Duration = Duration::from_secs(60);
pub(crate) const CREDENTIAL_CACHE_CAPACITY: usize = 256;

static IAM_CREDENTIALS_CACHE: OnceLock<Mutex<HashMap<String, CachedCredentials>>> = OnceLock::new();

#[derive(Clone)]
struct CachedCredentials {
    credentials: Credentials,
    expires_at: SystemTime,
}

pub(crate) fn credential_cache_ttl(flow: &AwsAuthFlow) -> Option<Duration> {
    match flow {
        AwsAuthFlow::StaticKeys { .. } => Some(STATIC_CREDENTIALS_TTL),
        AwsAuthFlow::DefaultChain => Some(AMBIENT_CREDENTIALS_TTL),
        AwsAuthFlow::WebIdentity { .. }
        | AwsAuthFlow::AssumeRole { .. }
        | AwsAuthFlow::Profile { .. }
        | AwsAuthFlow::SessionToken { .. } => None,
    }
}

pub(crate) fn cache_key(config: &AwsAuthConfig, flow: &AwsAuthFlow) -> String {
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

pub(crate) fn get_cached_credentials(key: &str) -> Option<Credentials> {
    let cache = IAM_CREDENTIALS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut entries = cache.lock().ok()?;
    let entry = entries.get(key)?;
    if entry.expires_at > SystemTime::now() {
        return Some(entry.credentials.clone());
    }
    entries.remove(key);
    None
}

pub(crate) fn set_cached_credentials(key: String, credentials: Credentials, ttl: Duration) {
    set_cached_credentials_at(key, credentials, ttl, SystemTime::now());
}

pub(crate) fn set_cached_credentials_at(
    key: String,
    credentials: Credentials,
    ttl: Duration,
    now: SystemTime,
) {
    let cache = IAM_CREDENTIALS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut entries) = cache.lock() {
        insert_cached_credentials(&mut entries, key, credentials, ttl, now);
    }
}

fn insert_cached_credentials(
    entries: &mut HashMap<String, CachedCredentials>,
    key: String,
    credentials: Credentials,
    ttl: Duration,
    now: SystemTime,
) {
    let Some(policy_expiry) = now.checked_add(ttl) else {
        return;
    };
    let expires_at = credentials
        .expiry()
        .and_then(|expiry| expiry.checked_sub(CREDENTIAL_EXPIRY_SAFETY_WINDOW))
        .map_or(policy_expiry, |expiry| expiry.min(policy_expiry));
    if expires_at <= now {
        return;
    }
    entries.retain(|_, entry| entry.expires_at > now);
    if entries.len() >= CREDENTIAL_CACHE_CAPACITY && !entries.contains_key(&key) {
        let eviction_key = entries
            .iter()
            .min_by_key(|(_, entry)| entry.expires_at)
            .map(|(key, _)| key.clone());
        if let Some(eviction_key) = eviction_key {
            entries.remove(&eviction_key);
        }
    }
    entries.insert(
        key,
        CachedCredentials {
            credentials,
            expires_at,
        },
    );
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

pub(crate) fn same_role_arns(target: &str, caller: &str) -> bool {
    role_identity(target) == role_identity(caller)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials(expiry: Option<SystemTime>) -> Credentials {
        Credentials::new("ak", "sk", None, expiry, "test")
    }

    #[test]
    fn cache_deadline_is_capped_before_credential_expiry() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let credential_expiry = now + Duration::from_secs(300);
        let mut entries = HashMap::new();

        insert_cached_credentials(
            &mut entries,
            "key".to_string(),
            credentials(Some(credential_expiry)),
            Duration::from_secs(600),
            now,
        );

        assert_eq!(
            entries.get("key").map(|entry| entry.expires_at),
            Some(credential_expiry - CREDENTIAL_EXPIRY_SAFETY_WINDOW)
        );
    }

    #[test]
    fn cache_capacity_evicts_the_earliest_deadline() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let mut entries = HashMap::new();
        for index in 0..CREDENTIAL_CACHE_CAPACITY {
            insert_cached_credentials(
                &mut entries,
                format!("key-{index}"),
                credentials(None),
                Duration::from_secs(index as u64 + 1),
                now,
            );
        }

        insert_cached_credentials(
            &mut entries,
            "new-key".to_string(),
            credentials(None),
            Duration::from_secs(1_000),
            now,
        );

        assert_eq!(entries.len(), CREDENTIAL_CACHE_CAPACITY);
        assert!(!entries.contains_key("key-0"));
        assert!(entries.contains_key("new-key"));
    }

    #[test]
    fn cache_skips_credentials_inside_the_safety_window() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let mut entries = HashMap::new();

        insert_cached_credentials(
            &mut entries,
            "key".to_string(),
            credentials(Some(now + CREDENTIAL_EXPIRY_SAFETY_WINDOW)),
            Duration::from_secs(600),
            now,
        );

        assert!(entries.is_empty());
    }
}
