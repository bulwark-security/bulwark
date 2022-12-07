use http::method::InvalidMethod;

#[derive(thiserror::Error, Debug)]
pub enum FilterProcessingError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    InvalidMethod(#[from] InvalidMethod),
}
