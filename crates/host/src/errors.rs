/// Returned when an attempt to load a plugin fails.
#[derive(thiserror::Error, Debug)]
pub enum PluginLoadError {
    #[error(transparent)]
    WasiError(#[from] wasi_common::Error),
    #[error(transparent)]
    StringArray(#[from] wasi_common::StringArrayError),
    #[error(transparent)]
    CreatePool(#[from] deadpool_redis::CreatePoolError),
    #[error(transparent)]
    Resolution(#[from] bulwark_config::ResolutionError),
    #[error("at least one resource required")]
    ResourceMissing,
    #[error(transparent)]
    AnyError(#[from] anyhow::Error),
}

/// Returned when an attempt to instantiate a plugin fails.
#[derive(thiserror::Error, Debug)]
pub enum PluginInstantiationError {
    #[error(transparent)]
    WasiError(#[from] wasi_common::Error),
    #[error(transparent)]
    StringArray(#[from] wasi_common::StringArrayError),
    #[error(transparent)]
    ContextInstantiation(#[from] ContextInstantiationError),
    #[error(transparent)]
    AnyError(#[from] anyhow::Error),
}

/// Returned when an attempt to execute a function within a plugin environment fails.
#[derive(thiserror::Error, Debug)]
pub enum PluginExecutionError {
    #[error(transparent)]
    HandlerError(#[from] crate::bindings::exports::bulwark::plugin::http_handlers::Error),
    #[error(transparent)]
    WasiError(#[from] wasi_common::Error),
    #[error(transparent)]
    StringArray(#[from] wasi_common::StringArrayError),
    #[error("function not implemented '{expected}'")]
    NotImplementedError { expected: String },
    #[error(transparent)]
    AnyError(#[from] anyhow::Error),
}

/// Returned when attempting to create a [`PluginCtx`](crate::PluginCtx) fails.
#[derive(thiserror::Error, Debug)]
pub enum ContextInstantiationError {
    #[error(transparent)]
    StringArray(#[from] wasi_common::StringArrayError),
    #[error(transparent)]
    ConfigSerialization(#[from] bulwark_config::ConfigSerializationError),
    #[error(transparent)]
    AnyError(#[from] anyhow::Error),
}
