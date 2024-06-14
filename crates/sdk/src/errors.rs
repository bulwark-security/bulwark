/// Generic error
pub type Error = ::anyhow::Error;
pub use ::anyhow::anyhow as error;

/// Returned when an attempt to parse a counter within a plugin environment fails.
#[derive(thiserror::Error, Debug)]
pub enum ParseCounterError {
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
}

/// Returned when there is an issue with the remote state requested by the plugin.
#[derive(thiserror::Error, Debug)]
pub enum RemoteStateError {
    #[error("access to state key '{key}' denied")]
    Permission { key: String },
    #[error("error accessing remote state: {message}")]
    Remote { message: String },
    #[error("invalid argument: {message}")]
    InvalidArgument { message: String },
    #[error("unexpected type received")]
    TypeError,
    #[error("{message}")]
    InvalidUnicode { message: String },
    #[error("could not parse integer value: {message}")]
    InvalidInteger { message: String },
    #[error("unexpected remote state error: {message}")]
    Other { message: String },
}

impl From<crate::wit::bulwark::plugin::redis::Error> for RemoteStateError {
    fn from(error: crate::wit::bulwark::plugin::redis::Error) -> Self {
        match error {
            crate::wit::bulwark::plugin::redis::Error::Permission(key) => {
                RemoteStateError::Permission { key }
            }
            crate::wit::bulwark::plugin::redis::Error::Remote(message) => {
                RemoteStateError::Remote { message }
            }
            crate::wit::bulwark::plugin::redis::Error::InvalidArgument(message) => {
                RemoteStateError::InvalidArgument { message }
            }
            crate::wit::bulwark::plugin::redis::Error::TypeError => RemoteStateError::TypeError,
            crate::wit::bulwark::plugin::redis::Error::Other(message) => {
                RemoteStateError::Other { message }
            }
        }
    }
}

impl From<std::string::FromUtf8Error> for RemoteStateError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        RemoteStateError::InvalidUnicode {
            message: error.to_string(),
        }
    }
}

impl From<ParseCounterError> for RemoteStateError {
    fn from(error: ParseCounterError) -> Self {
        RemoteStateError::InvalidInteger {
            message: error.to_string(),
        }
    }
}
