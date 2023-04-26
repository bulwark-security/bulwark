use bulwark_wasm_host::{ContextInstantiationError, PluginInstantiationError};
use http::method::InvalidMethod;

/// Returned when trying to instantiate a plugin group and either the request context for a plugin or the plugin
/// itself returns an instantiation error.
#[derive(thiserror::Error, Debug)]
pub enum PluginGroupInstantiationError {
    #[error(transparent)]
    ContextInstantiation(#[from] ContextInstantiationError),
    #[error(transparent)]
    PluginInstantiation(#[from] PluginInstantiationError),
}

/// Returned when trying to assemble a [`Request`](bulwark_wasm_sdk::Request) struct and Envoy sends missing
/// or invalid information or an [HTTP error](http::Error) occurs.
#[derive(thiserror::Error, Debug)]
pub enum PrepareRequestError {
    #[error(transparent)]
    InvalidMethod(#[from] InvalidMethod),
    #[error(transparent)]
    Http(#[from] http::Error),
    #[error("missing http method pseudo-header")]
    MissingMethod,
    #[error("missing http scheme pseudo-header")]
    MissingScheme,
    #[error("missing http authority pseudo-header")]
    MissingAuthority,
    #[error("missing http path pseudo-header")]
    MissingPath,
    #[error("missing envoy headers")]
    MissingHeaders,
}

/// Returned when trying to assemble a [`Request`](bulwark_wasm_sdk::Response) struct and Envoy sends missing
/// or invalid information or an [HTTP error](http::Error) occurs.
#[derive(thiserror::Error, Debug)]
pub enum PrepareResponseError {
    #[error(transparent)]
    Http(#[from] http::Error),
    #[error("missing http status pseudo-header")]
    MissingStatus,
    #[error("missing envoy headers")]
    MissingHeaders,
}
