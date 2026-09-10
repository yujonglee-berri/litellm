pub mod audio_transcription;
pub mod auth;
pub mod caching;
pub mod call_lifecycle;
pub mod chat_completions;
pub mod constants;
pub mod error;
pub mod http_utils;
mod media;
pub mod messages;
#[cfg(any(feature = "observability", test))]
pub mod observability;
pub mod ocr;
pub mod operation;
pub mod providers;
pub mod realtime;
pub mod responses;
pub mod routing_utils;
mod tls;
mod url_utils;

#[cfg(test)]
mod provider_operation_smoke_tests;

pub use auth::AuthError;
pub use error::Error;
