/// This error will be returned if an attempt to load Bulwark's configuration file fails.
#[derive(thiserror::Error, Debug)]
pub enum ConfigFileError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Deserialization(#[from] toml::de::Error),
    #[error(transparent)]
    Validation(#[from] validator::ValidationError),
    #[error(transparent)]
    Validations(#[from] validator::ValidationErrors),
    #[error("missing parent: '{0}'")]
    MissingParent(String),
}

/// This error will be returned if an attempt to serialize a config structure fails.
#[derive(thiserror::Error, Debug)]
pub enum ConfigSerializationError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// This error will be returned if attempting to resolve references fails.
#[derive(thiserror::Error, Debug)]
pub enum ResolutionError {
    #[error("missing named plugin or preset: '{0}'")]
    Missing(String),
}
