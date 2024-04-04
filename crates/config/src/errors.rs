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
    #[error(transparent)]
    Resolution(#[from] ResolutionError),
    #[error("missing parent: '{0}'")]
    MissingParent(String),
    #[error("invalid circular include: '{0}'")]
    CircularInclude(String),
    #[error("duplicate named plugin or preset: '{0}'")]
    Duplicate(String),
    #[error("invalid plugin config: {0}")]
    InvalidPluginConfig(String),
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
    #[error("invalid circular preset reference: '{0}'")]
    CircularPreset(String),
}
