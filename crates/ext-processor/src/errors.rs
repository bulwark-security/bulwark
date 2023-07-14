use bulwark_wasm_host::{ContextInstantiationError, PluginInstantiationError};

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
    InvalidMethod(#[from] http::method::InvalidMethod),
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
    #[error("no headers received from envoy")]
    MissingHeaders,
}

/// Returned when trying to assemble a [`Response`](bulwark_wasm_sdk::Response) struct and Envoy sends missing
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

/// Returned when serializing tags or [`Decision`](bulwark_wasm_sdk::Decision) values into [SFV](sfv).
#[derive(thiserror::Error, Debug)]
pub enum SfvError {
    #[error("could not serialize to structured field value: {0}")]
    Serialization(String),
}

/// Returned when performing an action that sends a
/// [`ProcessingRequest`](envoy_control_plane::envoy::service::ext_proc::v3::ProcessingRequest) or a
/// [`ProcessingResponse`](envoy_control_plane::envoy::service::ext_proc::v3::ProcessingResponse).
#[derive(thiserror::Error, Debug)]
pub enum ProcessingMessageError {
    #[error(transparent)]
    Send(#[from] futures::channel::mpsc::SendError),
    #[error(transparent)]
    Sfv(#[from] SfvError),
}
