use wasi_common::StringArrayError;

/// Returned when an attempt to load a plugin fails.
#[derive(thiserror::Error, Debug)]
pub enum PluginLoadError {
    #[error(transparent)]
    Error(#[from] anyhow::Error), // TODO: eliminate if possible
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
    #[error("at least one resource required")]
    ResourceMissing,
}

/// Returned when an attempt to instantiate a plugin fails.
#[derive(thiserror::Error, Debug)]
pub enum PluginInstantiationError {
    #[error(transparent)]
    Error(#[from] anyhow::Error), // TODO: eliminate if possible
    #[error(transparent)]
    ContextInstantiation(#[from] ContextInstantiationError),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}

/// Returned when an attempt to execute a function within a plugin environment fails.
#[derive(thiserror::Error, Debug)]
pub enum PluginExecutionError {
    #[error(transparent)]
    Error(#[from] anyhow::Error), // TODO: eliminate if possible
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
    #[error("function not implemented '{expected:?}'")]
    NotImplementedError { expected: String },
}

/// Returned when attempting to create a [`RequestContext`](crate::RequestContext) fails.
#[derive(thiserror::Error, Debug)]
pub enum ContextInstantiationError {
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}
