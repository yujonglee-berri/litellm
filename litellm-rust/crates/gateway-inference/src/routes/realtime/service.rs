//! Deployment selection adapter for the Realtime operation runtime.

use std::time::Duration;

use futures_util::{Sink, Stream};
use litellm_gateway_router::Router;
use litellm_operation_realtime::Error;
use litellm_operation_realtime::pool::RealtimePool;
use litellm_operation_realtime::runtime::{SessionConfig, run_session};
use litellm_operation_realtime::wire::RealtimeEvent;

/// Select a deployment for `model` and splice the client stream to the provider.
///
/// `pool` supplies a pre-warmed upstream when one is available; otherwise we
/// fresh-dial. A disabled pool always misses, so this collapses to the original
/// fresh-dial behavior.
pub async fn run<In, Out>(
    router: &Router,
    pool: &RealtimePool,
    model: &str,
    idle_timeout: Option<Duration>,
    observe: impl FnMut(&RealtimeEvent) + Send,
    client_in: In,
    client_out: Out,
) -> Result<(), Error>
where
    In: Stream<Item = RealtimeEvent> + Unpin + Send,
    Out: Sink<RealtimeEvent> + Unpin + Send,
    <Out as Sink<RealtimeEvent>>::Error: std::fmt::Display,
{
    let deployment = router
        .get_available_deployment(model)
        .ok_or_else(|| Error::Routing(format!("no deployment available for model '{model}'")))?;
    let params = &deployment.litellm_params;
    run_session(
        pool,
        SessionConfig {
            model: &params.model,
            api_key: params.api_key.as_deref(),
            api_base: params.api_base.as_deref(),
            idle_timeout,
        },
        observe,
        client_in,
        client_out,
    )
    .await
}
