#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid value for {name}: {value}")]
    InvalidEnvironment { name: &'static str, value: String },
    #[error("failed to build OTLP exporter: {0}")]
    Exporter(#[from] opentelemetry_otlp::ExporterBuildError),
    #[error("failed to install OpenTelemetry subscriber: {0}")]
    Subscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
}
