//! LiteLLM inference gateway library.
//!
//! Axum routes and gateway-hosted integrations for inference traffic.

#[cfg(feature = "server")]
pub mod routes;
#[cfg(feature = "server")]
pub mod state;
#[cfg(feature = "trace-parity")]
pub mod trace_parity;

mod constants;
pub mod integrations;
#[cfg(feature = "server")]
mod realtime;
