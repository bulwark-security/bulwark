use wasi_common::StringArrayError;

#[derive(thiserror::Error, Debug)]
pub enum PluginLoadError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}

#[derive(thiserror::Error, Debug)]
pub enum PluginInstantiationError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}

#[derive(thiserror::Error, Debug)]
pub enum PluginExecutionError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}
