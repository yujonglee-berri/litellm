use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use litellm_auth::{
    AuthError, CachedTokenProvider, ExistingHeaderBehavior, ResolvedCredential, SecretValue,
    TokenAuthResolver, TokenFuture, TokenProvider, TokenProviderHandle,
};
use reqwest::Url;
use serde::Deserialize;

use crate::constants::{
    AUTHORIZATION_CODE_GRANT, CLIENT_CREDENTIALS_GRANT, DEVICE_CODE_GRANT, REFRESH_TOKEN_GRANT,
    TOKEN_EXCHANGE_GRANT,
};
use crate::{Error, OAuthClientAuthentication, OAuthGrant};

const RESERVED_FORM_FIELDS: [&str; 13] = [
    "grant_type",
    "client_id",
    "client_secret",
    "refresh_token",
    "subject_token",
    "subject_token_type",
    "requested_token_type",
    "code",
    "redirect_uri",
    "code_verifier",
    "device_code",
    "scope",
    "audience",
];

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
    ) -> Result<Self, Error> {
        validate_endpoint(&endpoint)?;
        validate_extra_fields(&extra_fields)?;
        Ok(Self {
            client,
            endpoint,
            client_authentication,
            grant,
            extra_fields,
        })
    }

    pub async fn acquire_token(&self) -> Result<ResolvedCredential, Error> {
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
            .map_err(|error| Error::Request(error.without_url()))?;
        if !response.status().is_success() {
            return Err(Error::HttpStatus(response.status()));
        }
        let response = response
            .json::<TokenResponse>()
            .await
            .map_err(|error| Error::InvalidResponse(error.without_url()))?;
        if response.access_token.trim().is_empty() {
            return Err(Error::EmptyAccessToken);
        }
        let expires_on = response
            .expires_in
            .and_then(|seconds| SystemTime::now().checked_add(Duration::from_secs(seconds)));
        Ok(ResolvedCredential::AccessToken {
            token: SecretValue::new(response.access_token),
            expires_on,
        })
    }

    pub fn cached(self, refresh_before: Duration) -> TokenProviderHandle {
        let provider = TokenProviderHandle::new(Arc::new(self));
        TokenProviderHandle::new(Arc::new(CachedTokenProvider::new(provider, refresh_before)))
    }

    pub fn resolver(self, existing: ExistingHeaderBehavior) -> TokenAuthResolver {
        TokenAuthResolver::new(TokenProviderHandle::new(Arc::new(self)), existing)
    }

    pub fn cached_resolver(
        self,
        refresh_before: Duration,
        existing: ExistingHeaderBehavior,
    ) -> TokenAuthResolver {
        TokenAuthResolver::new(self.cached(refresh_before), existing)
    }

    fn form(&self) -> Vec<(String, String)> {
        grant_fields(&self.grant)
            .into_iter()
            .chain(client_fields(&self.client_authentication))
            .chain(
                self.extra_fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.expose().into())),
            )
            .collect()
    }
}

impl TokenProvider for OAuthTokenProvider {
    fn acquire(&self) -> TokenFuture<'_> {
        Box::pin(async move { self.acquire_token().await.map_err(auth_error) })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default, deserialize_with = "deserialize_optional_seconds")]
    expires_in: Option<u64>,
}

fn client_fields(authentication: &OAuthClientAuthentication) -> Vec<(String, String)> {
    match authentication {
        OAuthClientAuthentication::RequestBody {
            client_id,
            client_secret,
        } => std::iter::once(("client_id".into(), client_id.expose().into()))
            .chain(
                client_secret
                    .iter()
                    .map(|value| ("client_secret".into(), value.expose().into())),
            )
            .collect(),
        OAuthClientAuthentication::HttpBasic { .. } | OAuthClientAuthentication::None => Vec::new(),
    }
}

fn grant_fields(grant: &OAuthGrant) -> Vec<(String, String)> {
    match grant {
        OAuthGrant::ClientCredentials { scopes, audience } => {
            std::iter::once(("grant_type".into(), CLIENT_CREDENTIALS_GRANT.into()))
                .chain(scope_field(scopes))
                .chain(optional_field("audience", audience.as_deref()))
                .collect()
        }
        OAuthGrant::RefreshToken {
            refresh_token,
            scopes,
        } => vec![
            ("grant_type".into(), REFRESH_TOKEN_GRANT.into()),
            ("refresh_token".into(), refresh_token.expose().into()),
        ]
        .into_iter()
        .chain(scope_field(scopes))
        .collect(),
        OAuthGrant::AuthorizationCode {
            code,
            redirect_uri,
            code_verifier,
        } => vec![
            ("grant_type".into(), AUTHORIZATION_CODE_GRANT.into()),
            ("code".into(), code.expose().into()),
            ("redirect_uri".into(), redirect_uri.clone()),
        ]
        .into_iter()
        .chain(
            code_verifier
                .iter()
                .map(|value| ("code_verifier".into(), value.expose().into())),
        )
        .collect(),
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
        } => vec![
            ("grant_type".into(), TOKEN_EXCHANGE_GRANT.into()),
            ("subject_token".into(), subject_token.expose().into()),
            ("subject_token_type".into(), subject_token_type.clone()),
        ]
        .into_iter()
        .chain(optional_field(
            "requested_token_type",
            requested_token_type.as_deref(),
        ))
        .chain(optional_field("audience", audience.as_deref()))
        .chain(scope_field(scopes))
        .collect(),
    }
}

fn scope_field(scopes: &[String]) -> Option<(String, String)> {
    (!scopes.is_empty()).then(|| ("scope".into(), scopes.join(" ")))
}

fn optional_field(name: &str, value: Option<&str>) -> Option<(String, String)> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| (name.into(), value.into()))
}

fn validate_endpoint(endpoint: &Url) -> Result<(), Error> {
    if endpoint.scheme() != "https"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(Error::InvalidTokenEndpoint);
    }
    Ok(())
}

fn validate_extra_fields(extra_fields: &BTreeMap<String, SecretValue>) -> Result<(), Error> {
    match extra_fields
        .keys()
        .find(|name| RESERVED_FORM_FIELDS.contains(&name.as_str()))
    {
        Some(name) => Err(Error::ReservedExtraField(name.clone())),
        None => Ok(()),
    }
}

fn auth_error(error: Error) -> AuthError {
    AuthError::TokenAcquisition {
        mechanism: "OAuth",
        message: error.to_string(),
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    fn fields(grant: OAuthGrant) -> BTreeMap<String, String> {
        grant_fields(&grant).into_iter().collect()
    }

    fn local_provider(response: &'static str) -> OAuthTokenProvider {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });
        OAuthTokenProvider {
            client: reqwest::Client::new(),
            endpoint: Url::parse(&format!("http://{address}/token")).unwrap(),
            client_authentication: OAuthClientAuthentication::None,
            grant: OAuthGrant::ClientCredentials {
                scopes: Vec::new(),
                audience: None,
            },
            extra_fields: BTreeMap::new(),
        }
    }

    #[test]
    fn client_credentials_form_includes_scopes_and_audience() {
        assert_eq!(
            fields(OAuthGrant::ClientCredentials {
                scopes: vec!["scope:a".into(), "scope:b".into()],
                audience: Some("api".into()),
            }),
            BTreeMap::from([
                ("audience".into(), "api".into()),
                ("grant_type".into(), CLIENT_CREDENTIALS_GRANT.into()),
                ("scope".into(), "scope:a scope:b".into()),
            ])
        );
    }

    #[test]
    fn refresh_token_form_includes_refresh_token_and_scopes() {
        assert_eq!(
            fields(OAuthGrant::RefreshToken {
                refresh_token: SecretValue::new("refresh"),
                scopes: vec!["read".into(), "write".into()],
            }),
            BTreeMap::from([
                ("grant_type".into(), REFRESH_TOKEN_GRANT.into()),
                ("refresh_token".into(), "refresh".into()),
                ("scope".into(), "read write".into()),
            ])
        );
    }

    #[test]
    fn authorization_code_form_includes_pkce_fields() {
        assert_eq!(
            fields(OAuthGrant::AuthorizationCode {
                code: SecretValue::new("code"),
                redirect_uri: "https://client.example/callback".into(),
                code_verifier: Some(SecretValue::new("verifier")),
            }),
            BTreeMap::from([
                ("code".into(), "code".into()),
                ("code_verifier".into(), "verifier".into()),
                ("grant_type".into(), AUTHORIZATION_CODE_GRANT.into()),
                (
                    "redirect_uri".into(),
                    "https://client.example/callback".into(),
                ),
            ])
        );
    }

    #[test]
    fn device_code_form_uses_device_grant_type() {
        assert_eq!(
            fields(OAuthGrant::DeviceCode {
                device_code: SecretValue::new("device"),
            }),
            BTreeMap::from([
                ("device_code".into(), "device".into()),
                ("grant_type".into(), DEVICE_CODE_GRANT.into()),
            ])
        );
    }

    #[test]
    fn token_exchange_form_includes_all_supported_fields() {
        assert_eq!(
            fields(OAuthGrant::TokenExchange {
                subject_token: SecretValue::new("subject"),
                subject_token_type: "urn:token:jwt".into(),
                requested_token_type: Some("urn:token:access".into()),
                audience: Some("api".into()),
                scopes: vec!["read".into()],
            }),
            BTreeMap::from([
                ("audience".into(), "api".into()),
                ("grant_type".into(), TOKEN_EXCHANGE_GRANT.into()),
                ("requested_token_type".into(), "urn:token:access".into()),
                ("scope".into(), "read".into()),
                ("subject_token".into(), "subject".into()),
                ("subject_token_type".into(), "urn:token:jwt".into()),
            ])
        );
    }

    #[test]
    fn request_body_authentication_and_extra_fields_extend_grant_form() {
        let provider = OAuthTokenProvider {
            client: reqwest::Client::new(),
            endpoint: Url::parse("https://issuer.example/token").unwrap(),
            client_authentication: OAuthClientAuthentication::RequestBody {
                client_id: SecretValue::new("client"),
                client_secret: Some(SecretValue::new("secret")),
            },
            grant: OAuthGrant::ClientCredentials {
                scopes: Vec::new(),
                audience: None,
            },
            extra_fields: BTreeMap::from([("resource".into(), SecretValue::new("resource"))]),
        };

        assert_eq!(
            provider.form().into_iter().collect::<BTreeMap<_, _>>(),
            BTreeMap::from([
                ("client_id".into(), "client".into()),
                ("client_secret".into(), "secret".into()),
                ("grant_type".into(), CLIENT_CREDENTIALS_GRANT.into()),
                ("resource".into(), "resource".into()),
            ])
        );
    }

    #[test]
    fn empty_optional_form_values_are_omitted() {
        assert_eq!(
            fields(OAuthGrant::ClientCredentials {
                scopes: Vec::new(),
                audience: Some("  ".into()),
            }),
            BTreeMap::from([("grant_type".into(), CLIENT_CREDENTIALS_GRANT.into())])
        );
        assert_eq!(
            fields(OAuthGrant::AuthorizationCode {
                code: SecretValue::new("code"),
                redirect_uri: "https://client.example/callback".into(),
                code_verifier: None,
            }),
            BTreeMap::from([
                ("code".into(), "code".into()),
                ("grant_type".into(), AUTHORIZATION_CODE_GRANT.into()),
                (
                    "redirect_uri".into(),
                    "https://client.example/callback".into(),
                ),
            ])
        );
        assert_eq!(
            fields(OAuthGrant::TokenExchange {
                subject_token: SecretValue::new("subject"),
                subject_token_type: "urn:token:jwt".into(),
                requested_token_type: None,
                audience: None,
                scopes: Vec::new(),
            }),
            BTreeMap::from([
                ("grant_type".into(), TOKEN_EXCHANGE_GRANT.into()),
                ("subject_token".into(), "subject".into()),
                ("subject_token_type".into(), "urn:token:jwt".into()),
            ])
        );
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
            assert!(matches!(error, Error::InvalidTokenEndpoint));
        }
    }

    #[test]
    fn extra_fields_reject_reserved_names_but_allow_oauth_extensions() {
        for name in RESERVED_FORM_FIELDS {
            let error = OAuthTokenProvider::new(
                reqwest::Client::new(),
                Url::parse("https://issuer.example/token").unwrap(),
                OAuthClientAuthentication::None,
                OAuthGrant::ClientCredentials {
                    scopes: Vec::new(),
                    audience: None,
                },
                BTreeMap::from([(name.into(), SecretValue::new("override"))]),
            )
            .unwrap_err();
            assert!(matches!(
                error,
                Error::ReservedExtraField(field) if field == name
            ));
        }

        let provider = OAuthTokenProvider::new(
            reqwest::Client::new(),
            Url::parse("https://issuer.example/token").unwrap(),
            OAuthClientAuthentication::None,
            OAuthGrant::ClientCredentials {
                scopes: Vec::new(),
                audience: None,
            },
            BTreeMap::from([
                ("client_assertion".into(), SecretValue::new("assertion")),
                ("Grant_Type".into(), SecretValue::new("case-sensitive")),
            ]),
        )
        .unwrap();

        assert_eq!(
            provider.form().into_iter().collect::<BTreeMap<_, _>>(),
            BTreeMap::from([
                ("Grant_Type".into(), "case-sensitive".into()),
                ("client_assertion".into(), "assertion".into()),
                ("grant_type".into(), CLIENT_CREDENTIALS_GRANT.into()),
            ])
        );
    }

    #[tokio::test]
    async fn acquires_access_token_and_parses_string_expiration() {
        let provider = local_provider(concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: application/json\r\n",
            "Connection: close\r\n\r\n",
            r#"{"access_token":"token","expires_in":"120"}"#
        ));
        let earliest_expiration = SystemTime::now() + Duration::from_secs(119);

        let credential = provider.acquire_token().await.unwrap();

        let ResolvedCredential::AccessToken { token, expires_on } = credential else {
            panic!("OAuth must return an access token")
        };
        assert_eq!(token.expose(), "token");
        assert!(expires_on.unwrap() >= earliest_expiration);
    }

    #[tokio::test]
    async fn non_success_status_is_a_typed_oauth_error() {
        let provider = local_provider(concat!(
            "HTTP/1.1 401 Unauthorized\r\n",
            "Content-Length: 0\r\n",
            "Connection: close\r\n\r\n"
        ));

        let error = provider.acquire_token().await.unwrap_err();

        assert!(matches!(
            error,
            Error::HttpStatus(reqwest::StatusCode::UNAUTHORIZED)
        ));
    }

    #[tokio::test]
    async fn malformed_response_is_a_typed_oauth_error() {
        let provider = local_provider(concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: application/json\r\n",
            "Connection: close\r\n\r\n",
            "not-json"
        ));

        let error = provider.acquire_token().await.unwrap_err();

        assert!(matches!(error, Error::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn transport_failure_is_a_typed_oauth_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let provider = OAuthTokenProvider {
            client: reqwest::Client::new(),
            endpoint: Url::parse(&format!("http://{address}/token")).unwrap(),
            client_authentication: OAuthClientAuthentication::None,
            grant: OAuthGrant::ClientCredentials {
                scopes: Vec::new(),
                audience: None,
            },
            extra_fields: BTreeMap::new(),
        };

        let error = provider.acquire_token().await.unwrap_err();

        assert!(matches!(error, Error::Request(_)));
    }

    #[tokio::test]
    async fn token_provider_contract_maps_oauth_error_to_shared_auth_error() {
        let provider = local_provider(concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: application/json\r\n",
            "Connection: close\r\n\r\n",
            r#"{"access_token":""}"#
        ));

        let error = provider.acquire().await.unwrap_err();

        assert_eq!(
            error,
            AuthError::TokenAcquisition {
                mechanism: "OAuth",
                message: Error::EmptyAccessToken.to_string(),
            }
        );
    }

    #[test]
    fn constructs_shared_resolvers_for_direct_pipeline_injection() {
        let provider = OAuthTokenProvider {
            client: reqwest::Client::new(),
            endpoint: Url::parse("https://issuer.example/token").unwrap(),
            client_authentication: OAuthClientAuthentication::None,
            grant: OAuthGrant::ClientCredentials {
                scopes: Vec::new(),
                audience: None,
            },
            extra_fields: BTreeMap::new(),
        };

        let _: TokenAuthResolver = provider.clone().resolver(ExistingHeaderBehavior::Preserve);
        let _: TokenAuthResolver =
            provider.cached_resolver(Duration::from_secs(30), ExistingHeaderBehavior::Replace);
    }
}
