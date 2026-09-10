//! Compatibility re-exports for the operation-independent auth crates.

pub use litellm_auth::*;

pub mod error {
    pub use litellm_auth::error::*;
}

pub mod azure {
    pub use litellm_auth_azure::*;
}
