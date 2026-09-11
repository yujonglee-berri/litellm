//! AWS credential resolution and SigV4 signing shared by provider operations.

mod bedrock;
mod config;
pub mod constants;
mod credentials;
pub mod error;
mod resolver;
mod signing;

#[cfg(test)]
mod tests;

pub use bedrock::{
    aws_auth_config, bedrock_model_id_and_region, host_supplied_credentials, resolve_bedrock_region,
};
pub use config::{AwsAuthConfig, AwsAuthFlow, classify_auth};
pub use credentials::resolve_credentials;
pub use error::Error;
pub use resolver::{
    BedrockAuthContext, BedrockAuthInput, BedrockAuthKind, BedrockAuthResolver, BedrockAuthSource,
};
pub use signing::{
    AwsSigV4Auth, AwsSignableRequest, aws_signature_headers, is_sigv4_computed_header,
    sign_bedrock_post, sign_request,
};
