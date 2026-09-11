use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;

use crate::{Config, Error};

pub struct Runtime {
    provider: SdkTracerProvider,
}

impl Runtime {
    pub fn install(config: Config) -> Result<Self, Error> {
        let mut exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary);
        if let Some(endpoint) = config.traces_endpoint().or(config.endpoint()) {
            exporter = exporter.with_endpoint(endpoint);
        }
        if let Some(headers) = config.headers() {
            exporter = exporter.with_headers(parse_headers(headers));
        }
        let exporter = exporter.build()?;
        let resource = Resource::builder()
            .with_service_name(config.service_name().to_string())
            .build();
        let provider = SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer("litellm-gateway");
        let subscriber = tracing_subscriber::registry().with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_location(false),
        );

        tracing::subscriber::set_global_default(subscriber)?;
        global::set_text_map_propagator(TraceContextPropagator::new());

        Ok(Self { provider })
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.provider.shutdown();
    }
}

fn parse_headers(headers: &str) -> std::collections::HashMap<String, String> {
    headers
        .split(',')
        .filter_map(|header| header.trim().split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_headers;

    #[test]
    fn parses_standard_otel_header_list() {
        assert_eq!(
            parse_headers("authorization=Bearer%20token, x-tenant=team-1"),
            std::collections::HashMap::from([
                ("authorization".to_string(), "Bearer%20token".to_string()),
                ("x-tenant".to_string(), "team-1".to_string()),
            ])
        );
    }
}
