use wasi_common::StringArrayError;

#[derive(thiserror::Error, Debug)]
pub enum PluginLoadError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
    #[error("at least one resource required")]
    ResourceMissing,
}

#[derive(thiserror::Error, Debug)]
pub enum PluginInstantiationError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    ContextInstantiation(#[from] ContextInstantiationError),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}

#[derive(thiserror::Error, Debug)]
pub enum PluginExecutionError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
    #[error("function not implemented '{expected:?}'")]
    NotImplementedError { expected: String },
}

#[derive(thiserror::Error, Debug)]
pub enum ContextInstantiationError {
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}
