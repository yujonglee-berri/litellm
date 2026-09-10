//! Authentication primitives shared by LiteLLM gateway HTTP routes.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Provides the configured gateway master key to [`RequireMasterKey`].
pub trait MasterKeyProvider {
    /// Returns the configured master key, or `None` when gateway auth is not
    /// configured.
    fn master_key(&self) -> Option<&str>;
}

/// SHA-256 hex digest of a token — the exact transform the Python proxy applies
/// (`litellm.proxy.utils.hash_token`).
///
/// A raw key must never leave the gateway in a log payload. Spend logs and
/// callback integrations receive `user_api_key_hash`, so that field must be
/// this hash rather than the credential.
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Extractor that requires the configured master key as a bearer token.
///
/// Rejections: `500` when no master key is configured (permanent
/// misconfiguration, not a transient outage); `401` on a missing or incorrect
/// token. The comparison is constant-time.
pub struct RequireMasterKey;

#[axum::async_trait]
impl<S> FromRequestParts<S> for RequireMasterKey
where
    S: MasterKeyProvider + Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Some(expected) = state.master_key() else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "gateway auth not configured (set LITELLM_MASTER_KEY)".to_string(),
            ));
        };
        let provided = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim);
        match provided {
            Some(token) if bool::from(token.as_bytes().ct_eq(expected.as_bytes())) => Ok(Self),
            _ => Err((
                StatusCode::UNAUTHORIZED,
                "missing or invalid bearer token".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hash_token;

    #[test]
    fn hash_token_matches_python_sha256_hexdigest() {
        assert_eq!(
            hash_token("sk-1234"),
            "88dc28d0f030c55ed4ab77ed8faf098196cb1c05df778539800c9f1243fe6b4b"
        );
        let hash = hash_token("sk-secret");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|character| character.is_ascii_hexdigit()));
        assert_ne!(hash, "sk-secret");
    }
}
