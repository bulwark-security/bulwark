#[derive(thiserror::Error, Debug)]
pub enum CliArgumentError {
    #[error("invalid log format: {0}")]
    InvalidLogFormat(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    #[error("error starting envoy external processor service: {0}")]
    ExtProcessorService(tonic::transport::Error),
    #[error("error starting admin service: {0}")]
    AdminService(hyper::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("missing file '{0}': {1}")]
    NotFound(String, std::io::Error),
    #[error("missing parent directory")]
    MissingParent,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("error reading plugin metadata: {0}")]
    CargoMetadata(#[from] cargo_metadata::Error),
    #[error("missing plugin metadata")]
    MissingMetadata,
    #[error("error adapting wasm: {0}")]
    Adapter(String),
}

#[derive(thiserror::Error, Debug)]
pub enum AdminServiceError {}
