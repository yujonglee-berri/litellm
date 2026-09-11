use litellm_auth::{ResolvedCredential, SecretValue, TokenProviderHandle};

#[derive(Clone, Debug)]
pub enum GoogleCredentialSource {
    AccessToken(ResolvedCredential),
    ServiceAccountJson(SecretValue),
    ApplicationDefault,
    AuthorizedUserJson(SecretValue),
    ExternalAccountJson(SecretValue),
    AwsWorkloadIdentity(SecretValue),
    PluggableExternalAccount(SecretValue),
    Caller(TokenProviderHandle),
}

#[derive(Clone, Debug)]
pub struct GoogleTokenRequest {
    pub source: GoogleCredentialSource,
    pub scopes: Vec<String>,
    pub audience: Option<String>,
    pub quota_project_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GoogleAuthInputs {
    pub token_request: GoogleTokenRequest,
    pub project: Option<String>,
    pub location: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GoogleAuthContext {
    pub project: Option<String>,
    pub location: Option<String>,
}
