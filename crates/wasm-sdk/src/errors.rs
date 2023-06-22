/// Generic result
pub type Result = ::std::result::Result<(), Error>;

/// Generic error
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Returned when an attempt to parse a counter within a plugin environment fails.
    #[error(transparent)]
    ParseCounter(#[from] ParseCounterError),
    /// Returned when an attempt to convert to a [`String`] fails due to invalid utf8 byte sequences.
    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),
    /// Returned when an attempt to access a resource that requires a permission fails.
    #[error(transparent)]
    Permission(#[from] PermissionError),
    /// Returned when there is an issue with the environment variable requested by the plugin.
    #[error(transparent)]
    EnvVar(#[from] EnvVarError),
    /// Returned when there can only be a single validation error
    #[error(transparent)]
    Validation(#[from] validator::ValidationError),
    /// Returned when there could be multiple validation errors
    #[error(transparent)]
    Validations(#[from] validator::ValidationErrors),
    /// Catch-all error type
    #[error(transparent)]
    Any(#[from] anyhow::Error),
}

impl From<crate::bulwark_host::EnvError> for Error {
    fn from(error: crate::bulwark_host::EnvError) -> Self {
        match error {
            crate::bulwark_host::EnvError::Permission(var) => {
                Error::Permission(PermissionError::Environment { var })
            }
            crate::bulwark_host::EnvError::Missing(var) => {
                Error::EnvVar(EnvVarError::Missing { var })
            }
            crate::bulwark_host::EnvError::NotUnicode(var) => {
                Error::EnvVar(EnvVarError::NotUnicode { var })
            }
        }
    }
}

/// Returned when an attempt to parse a counter within a plugin environment fails.
#[derive(thiserror::Error, Debug)]
pub enum ParseCounterError {
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
}

/// Returned when an attempt to access a resource that requires a permission fails.
#[derive(thiserror::Error, Debug)]
pub enum PermissionError {
    #[error("access to environment variable '{var}' denied")]
    Environment { var: String },
    #[error("access to http host '{host}' denied")]
    Http { host: String },
    #[error("access to state key '{key}' denied")]
    State { key: String },
}

/// Returned when there is an issue with the environment variable requested by the plugin.
#[derive(thiserror::Error, Debug)]
pub enum EnvVarError {
    #[error("environment variable '{var}' missing")]
    Missing { var: String },
    #[error("environment variable '{var}' was not unicode")]
    NotUnicode { var: String },
}
