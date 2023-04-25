/// This error will be returned if an attempt to load Bulwark's configuration file fails.
#[derive(thiserror::Error, Debug)]
pub enum ConfigFileError {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    DeserializationError(#[from] toml::de::Error),
}
