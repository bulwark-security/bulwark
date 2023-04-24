#[derive(thiserror::Error, Debug)]
pub enum CliArgumentError {
    #[error("invalid log format: {0}")]
    InvalidLogFormat(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    #[error("error starting envoy external filter service: {0}")]
    ExtFilterService(tonic::transport::Error),
    #[error("error starting admin service: {0}")]
    AdminService(hyper::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum AdminServiceError {}
