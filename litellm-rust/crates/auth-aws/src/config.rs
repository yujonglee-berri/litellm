use std::fmt;

use crate::constants::{
    AWS_ACCESS_KEY_ID, AWS_EXTERNAL_ID, AWS_PROFILE_NAME, AWS_REGION_NAME, AWS_ROLE_NAME,
    AWS_SECRET_ACCESS_KEY, AWS_SESSION_NAME, AWS_SESSION_TOKEN, AWS_STS_ENDPOINT,
    AWS_WEB_IDENTITY_TOKEN,
};

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

impl AwsAuthConfig {
    pub(crate) fn with_environment(
        self,
        env_lookup: &(dyn Fn(&str) -> Option<String> + Sync),
    ) -> Self {
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
