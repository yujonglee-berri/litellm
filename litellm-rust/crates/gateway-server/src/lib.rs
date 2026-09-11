mod server;
mod state;

pub use server::app;
#[cfg(feature = "otel")]
pub use server::app_with_otel;
pub use state::AppState;
