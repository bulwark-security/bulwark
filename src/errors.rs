use std::convert::From;

#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    #[error("error starting envoy external filter service: {0}")]
    ExtFilterServiceError(tonic::transport::Error),
    #[error("error starting admin service: {0}")]
    AdminServiceError(hyper::Error),
}
