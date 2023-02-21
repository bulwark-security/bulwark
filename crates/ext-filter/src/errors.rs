use bulwark_wasm_host::{ContextInstantiationError, PluginInstantiationError};
use http::method::InvalidMethod;

#[derive(thiserror::Error, Debug)]
pub enum MultiPluginInstantiationError {
    #[error(transparent)]
    ContextInstantiation(#[from] ContextInstantiationError),
    #[error(transparent)]
    PluginInstantiation(#[from] PluginInstantiationError),
}

#[derive(thiserror::Error, Debug)]
pub enum FilterProcessingError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    InvalidMethod(#[from] InvalidMethod),
}
