use std::time::Duration;

use futures_util::{Sink, Stream};

use crate::Error;
use crate::client::{realtime, realtime_warm, resolve_model};
use crate::pool::{RealtimePool, upstream_key};
use crate::wire::RealtimeEvent;

#[derive(Clone, Copy, Debug)]
pub struct SessionConfig<'a> {
    pub model: &'a str,
    pub api_key: Option<&'a str>,
    pub api_base: Option<&'a str>,
    pub idle_timeout: Option<Duration>,
}

pub async fn run_session<In, Out>(
    pool: &RealtimePool,
    config: SessionConfig<'_>,
    observe: impl FnMut(&RealtimeEvent) + Send,
    client_in: In,
    client_out: Out,
) -> Result<(), Error>
where
    In: Stream<Item = RealtimeEvent> + Unpin + Send,
    Out: Sink<RealtimeEvent> + Unpin + Send,
    <Out as Sink<RealtimeEvent>>::Error: std::fmt::Display,
{
    let model = resolve_model(config.model)?;
    if let Ok(key) = upstream_key(model, config.api_key, config.api_base)
        && let Some(handoff) = pool.take(&key)
    {
        return realtime_warm(
            model,
            handoff,
            config.idle_timeout,
            observe,
            client_in,
            client_out,
        )
        .await;
    }

    realtime(
        model,
        config.api_key,
        config.api_base,
        config.idle_timeout,
        observe,
        client_in,
        client_out,
    )
    .await
}
