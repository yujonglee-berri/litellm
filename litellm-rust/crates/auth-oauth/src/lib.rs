//! Typed OAuth token exchanges.
//!
//! Browser interaction and durable refresh-token storage belong to the host. This crate owns
//! non-interactive token endpoint exchanges and exposes the result through `TokenProvider`.

mod constants;

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use litellm_auth::error::AuthConfigurationError;
use litellm_auth::{
    AuthError, CachedTokenProvider, ResolvedCredential, SecretValue, TokenFuture, TokenProvider,
    TokenProviderHandle,
};
use reqwest::Url;
use serde::Deserialize;

use constants::{
    AUTHORIZATION_CODE_GRANT, CLIENT_CREDENTIALS_GRANT, DEVICE_CODE_GRANT, REFRESH_TOKEN_GRANT,
    TOKEN_EXCHANGE_GRANT,
};

#[derive(Clone, Debug)]
pub enum OAuthClientAuthentication {
    RequestBody {
        client_id: SecretValue,
        client_secret: Option<SecretValue>,
    },
    HttpBasic {
        client_id: SecretValue,
        client_secret: SecretValue,
    },
    None,
}

#[derive(Clone, Debug)]
pub enum OAuthGrant {
    ClientCredentials {
        scopes: Vec<String>,
        audience: Option<String>,
    },
    RefreshToken {
        refresh_token: SecretValue,
        scopes: Vec<String>,
    },
    AuthorizationCode {
        code: SecretValue,
        redirect_uri: String,
        code_verifier: Option<SecretValue>,
    },
    DeviceCode {
        device_code: SecretValue,
    },
    TokenExchange {
        subject_token: SecretValue,
        subject_token_type: String,
        requested_token_type: Option<String>,
        audience: Option<String>,
        scopes: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub struct OAuthTokenProvider {
    client: reqwest::Client,
    endpoint: Url,
    client_authentication: OAuthClientAuthentication,
    grant: OAuthGrant,
    extra_fields: BTreeMap<String, SecretValue>,
}

impl OAuthTokenProvider {
    pub fn new(
        client: reqwest::Client,
        endpoint: Url,
        client_authentication: OAuthClientAuthentication,
        grant: OAuthGrant,
        extra_fields: BTreeMap<String, SecretValue>,
    ) -> Result<Self, AuthError> {
        validate_endpoint(&endpoint)?;
        Ok(Self {
            client,
            endpoint,
            client_authentication,
            grant,
            extra_fields,
        })
    }

    pub fn cached(self, refresh_before: Duration) -> TokenProviderHandle {
        let provider = TokenProviderHandle::new(std::sync::Arc::new(self));
        TokenProviderHandle::new(std::sync::Arc::new(CachedTokenProvider::new(
            provider,
            refresh_before,
        )))
    }

    fn form(&self) -> Vec<(String, String)> {
        let mut fields = grant_fields(&self.grant);
        if let OAuthClientAuthentication::RequestBody {
            client_id,
            client_secret,
        } = &self.client_authentication
        {
            fields.push(("client_id".into(), client_id.expose().into()));
            if let Some(client_secret) = client_secret {
                fields.push(("client_secret".into(), client_secret.expose().into()));
            }
        }
        fields.extend(
            self.extra_fields
                .iter()
                .map(|(name, value)| (name.clone(), value.expose().into())),
        );
        fields
    }
}

impl TokenProvider for OAuthTokenProvider {
    fn acquire(&self) -> TokenFuture<'_> {
        Box::pin(async move {
            let request = self.client.post(self.endpoint.clone()).form(&self.form());
            let request = match &self.client_authentication {
                OAuthClientAuthentication::HttpBasic {
                    client_id,
                    client_secret,
                } => request.basic_auth(client_id.expose(), Some(client_secret.expose())),
                OAuthClientAuthentication::RequestBody { .. } | OAuthClientAuthentication::None => {
                    request
                }
            };
            let response = request
                .send()
                .await
                .map_err(|error| token_error(error.without_url()))?;
            if !response.status().is_success() {
                return Err(token_error(format!("HTTP {}", response.status())));
            }
            let response: TokenResponse = response
                .json()
                .await
                .map_err(|error| token_error(error.without_url()))?;
            if response.access_token.trim().is_empty() {
                return Err(token_error("token endpoint returned an empty access token"));
            }
            let expires_on = response
                .expires_in
                .and_then(|seconds| SystemTime::now().checked_add(Duration::from_secs(seconds)));
            Ok(ResolvedCredential::AccessToken {
                token: SecretValue::new(response.access_token),
                expires_on,
            })
        })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default, deserialize_with = "deserialize_optional_seconds")]
    expires_in: Option<u64>,
}

fn grant_fields(grant: &OAuthGrant) -> Vec<(String, String)> {
    let mut fields = match grant {
        OAuthGrant::ClientCredentials { scopes, audience } => {
            let mut fields = vec![("grant_type".into(), CLIENT_CREDENTIALS_GRANT.into())];
            push_scopes(&mut fields, scopes);
            push_optional(&mut fields, "audience", audience.as_deref());
            fields
        }
        OAuthGrant::RefreshToken {
            refresh_token,
            scopes,
        } => {
            let mut fields = vec![
                ("grant_type".into(), REFRESH_TOKEN_GRANT.into()),
                ("refresh_token".into(), refresh_token.expose().into()),
            ];
            push_scopes(&mut fields, scopes);
            fields
        }
        OAuthGrant::AuthorizationCode {
            code,
            redirect_uri,
            code_verifier,
        } => {
            let mut fields = vec![
                ("grant_type".into(), AUTHORIZATION_CODE_GRANT.into()),
                ("code".into(), code.expose().into()),
                ("redirect_uri".into(), redirect_uri.clone()),
            ];
            if let Some(verifier) = code_verifier {
                fields.push(("code_verifier".into(), verifier.expose().into()));
            }
            fields
        }
        OAuthGrant::DeviceCode { device_code } => vec![
            ("grant_type".into(), DEVICE_CODE_GRANT.into()),
            ("device_code".into(), device_code.expose().into()),
        ],
        OAuthGrant::TokenExchange {
            subject_token,
            subject_token_type,
            requested_token_type,
            audience,
            scopes,
        } => {
            let mut fields = vec![
                ("grant_type".into(), TOKEN_EXCHANGE_GRANT.into()),
                ("subject_token".into(), subject_token.expose().into()),
                ("subject_token_type".into(), subject_token_type.clone()),
            ];
            push_optional(
                &mut fields,
                "requested_token_type",
                requested_token_type.as_deref(),
            );
            push_optional(&mut fields, "audience", audience.as_deref());
            push_scopes(&mut fields, scopes);
            fields
        }
    };
    fields.shrink_to_fit();
    fields
}

fn push_scopes(fields: &mut Vec<(String, String)>, scopes: &[String]) {
    if !scopes.is_empty() {
        fields.push(("scope".into(), scopes.join(" ")));
    }
}

fn push_optional(fields: &mut Vec<(String, String)>, name: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        fields.push((name.into(), value.into()));
    }
}

fn validate_endpoint(endpoint: &Url) -> Result<(), AuthError> {
    if endpoint.scheme() != "https"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(AuthConfigurationError::InvalidTokenEndpoint.into());
    }
    Ok(())
}

fn token_error(message: impl ToString) -> AuthError {
    AuthError::TokenAcquisition {
        mechanism: "OAuth",
        message: message.to_string(),
    }
}

fn deserialize_optional_seconds<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(number)) => number.as_u64(),
        Some(serde_json::Value::String(value)) => value.parse().ok(),
        Some(_) | None => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(grant: OAuthGrant) -> BTreeMap<String, String> {
        grant_fields(&grant).into_iter().collect()
    }

    #[test]
    fn every_supported_grant_has_an_explicit_wire_type() {
        let client_credentials = fields(OAuthGrant::ClientCredentials {
            scopes: vec!["scope:a".into(), "scope:b".into()],
            audience: Some("api".into()),
        });
        assert_eq!(client_credentials["grant_type"], CLIENT_CREDENTIALS_GRANT);
        assert_eq!(client_credentials["scope"], "scope:a scope:b");

        let token_exchange = fields(OAuthGrant::TokenExchange {
            subject_token: SecretValue::new("subject"),
            subject_token_type: "urn:token:jwt".into(),
            requested_token_type: None,
            audience: None,
            scopes: Vec::new(),
        });
        assert_eq!(token_exchange["grant_type"], TOKEN_EXCHANGE_GRANT);
        assert_eq!(token_exchange["subject_token"], "subject");
    }

    #[test]
    fn token_endpoint_rejects_credential_leakage_and_plaintext_transport() {
        for endpoint in [
            "http://issuer.example/token",
            "https://user:password@issuer.example/token",
            "https://issuer.example/token#secret",
        ] {
            let error = OAuthTokenProvider::new(
                reqwest::Client::new(),
                Url::parse(endpoint).unwrap(),
                OAuthClientAuthentication::None,
                OAuthGrant::ClientCredentials {
                    scopes: Vec::new(),
                    audience: None,
                },
                BTreeMap::new(),
            )
            .unwrap_err();
            assert_eq!(
                error,
                AuthError::Configuration(AuthConfigurationError::InvalidTokenEndpoint)
            );
        }
    }
}
