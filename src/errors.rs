#[derive(thiserror::Error, Debug)]
pub enum ConfigFileError {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    DeserializationError(#[from] toml::de::Error),
}
