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
}
