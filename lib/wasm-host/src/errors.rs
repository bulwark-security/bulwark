use wasi_common::StringArrayError;

#[derive(thiserror::Error, Debug)]
pub enum PluginLoadError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    StringArray(#[from] StringArrayError),
}
