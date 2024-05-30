/// This error will be returned if an attempt to load Bulwark's configuration file fails.
#[derive(thiserror::Error, Debug)]
pub enum ConfigFileError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("file not found: '{0}'")]
    NotFound(std::path::PathBuf),
    // TODO: when an include path within a config file doesn't exist
    // TODO: when a plugin .wasm path within a config file doesn't exist
    #[error(transparent)]
    Deserialization(#[from] toml::de::Error),
    #[error(transparent)]
    Validation(#[from] validator::ValidationError),
    #[error(transparent)]
    Validations(#[from] validator::ValidationErrors),
    #[error(transparent)]
    Resolution(#[from] ResolutionError),
    #[error(transparent)]
    InvalidRemoteUri(#[from] url::ParseError),
    #[error("uri must be https: '{0}'")]
    InsecureRemoteUri(String),
    #[error("missing parent: '{0}'")]
    MissingParent(String),
    #[error("invalid circular include: '{0}'")]
    CircularInclude(String),
    #[error("duplicate named plugin or preset: '{0}'")]
    Duplicate(String),
    #[error("invalid secret config: {0}")]
    InvalidSecretConfig(String),
    #[error("invalid plugin config: {0}")]
    InvalidPluginConfig(String),
    #[error("invalid resource config: {0}")]
    InvalidResourceConfig(String),
}

/// This error will be returned if an attempt to serialize a config structure fails.
#[derive(thiserror::Error, Debug)]
pub enum ConfigSerializationError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// This error will be returned if an attempt to convert a secret fails.
#[derive(thiserror::Error, Debug)]
pub enum SecretConversionError {
    #[error("one and only one of path or env_var must be set")]
    InvalidSecretLocation,
}

/// This error will be returned if an attempt to convert a plugin fails.
#[derive(thiserror::Error, Debug)]
pub enum PluginConversionError {
    #[error("one and only one of path, uri, or bytes must be set")]
    InvalidLocation,
    #[error(transparent)]
    InvalidRemoteUri(#[from] url::ParseError),
    #[error(transparent)]
    InvalidHexEncoding(#[from] hex::FromHexError),
}

/// This error will be returned if attempting to resolve references fails.
#[derive(thiserror::Error, Debug)]
pub enum ResolutionError {
    #[error("missing named plugin or preset: '{0}'")]
    Missing(String),
    #[error("invalid circular preset reference: '{0}'")]
    CircularPreset(String),
}
