#[derive(thiserror::Error, Debug)]
pub enum CliArgumentError {
    #[error("invalid log format: {0}")]
    InvalidLogFormat(String),
    #[error("missing subcommand")]
    MissingSubcommand,
}

#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    #[error("error starting envoy external processor service: {0}")]
    ExtProcessorService(#[from] tonic::transport::Error),
    #[error("error starting admin service: {0}")]
    AdminService(#[from] std::io::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum MetricsError {
    #[error("failed to install Prometheus metrics exporter: {0}")]
    Prometheus(#[from] metrics_exporter_prometheus::BuildError),
    #[error("failed to install StatsD metrics exporter: {0}")]
    Statsd(#[from] metrics_exporter_statsd::StatsdError),
    #[error("failed to install StatsD metrics exporter: {0}")]
    SetStatsd(#[from] metrics::SetRecorderError<metrics_exporter_statsd::StatsdRecorder>),
}

#[derive(thiserror::Error, Debug)]
pub enum AdminServiceError {}
