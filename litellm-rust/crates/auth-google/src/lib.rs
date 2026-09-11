//! Google and Vertex credential resolution.

mod error;
mod provider;
mod resolver;
mod types;

pub use error::Error;
pub use provider::{GoogleTokenAcquirer, GoogleTokenProvider, UnsupportedGoogleBackend};
pub use resolver::{GoogleAuthResolver, GoogleAuthSource};
pub use types::{GoogleAuthContext, GoogleAuthInputs, GoogleCredentialSource, GoogleTokenRequest};
