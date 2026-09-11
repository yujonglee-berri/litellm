use litellm_auth::SecretValue;

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
