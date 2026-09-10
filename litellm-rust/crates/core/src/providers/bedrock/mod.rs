//! User-directed exception: this base provider owns AWS auth I/O for parity
//! with Python's `BaseAWSLLM`; the broader core purity guidance is reconciled
//! separately.

#[cfg(feature = "bedrock-auth")]
pub mod audio_transcription;
#[cfg(feature = "bedrock-auth")]
pub use litellm_auth_aws as aws_base;

#[cfg(feature = "bedrock-auth")]
pub(crate) mod constants {
    pub use litellm_auth_aws::constants::*;
}
