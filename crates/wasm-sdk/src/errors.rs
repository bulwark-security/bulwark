/// Generic result
pub type Result = ::std::result::Result<(), Error>;

// TODO: error is way too big, replace w/ aliased anyhow or Box<dyn std::error::Error>

/// Generic error
pub type Error = ::anyhow::Error;

/// Returned when an attempt to parse a counter within a plugin environment fails.
#[derive(thiserror::Error, Debug)]
pub enum ParseCounterError {
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
}

/// Returned when there was an issue getting or setting a parameter.
#[derive(thiserror::Error, Debug)]
pub enum ParamError {
    #[error("{message}")]
    Json { message: String },
}

impl From<crate::bulwark_host::ParamError> for ParamError {
    fn from(error: crate::bulwark_host::ParamError) -> Self {
        match error {
            crate::bulwark_host::ParamError::Json(message) => ParamError::Json { message },
        }
    }
}

/// Returned when there is an issue with the environment variable requested by the plugin.
#[derive(thiserror::Error, Debug)]
pub enum EnvVarError {
    #[error("access to environment variable '{var}' denied")]
    Permission { var: String },
    #[error("environment variable '{var}' missing")]
    Missing { var: String },
    #[error("{message}")]
    NotUnicode { message: String },
}

impl From<crate::bulwark_host::EnvError> for EnvVarError {
    fn from(error: crate::bulwark_host::EnvError) -> Self {
        match error {
            crate::bulwark_host::EnvError::Permission(var) => EnvVarError::Permission { var },
            crate::bulwark_host::EnvError::Missing(var) => EnvVarError::Missing { var },
            crate::bulwark_host::EnvError::NotUnicode(var) => EnvVarError::NotUnicode {
                message: format!("environment variable '{var}' was not unicode"),
            },
        }
    }
}

impl From<std::string::FromUtf8Error> for EnvVarError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        EnvVarError::NotUnicode {
            message: error.to_string(),
        }
    }
}

/// Returned when there is an issue with the remote state requested by the plugin.
#[derive(thiserror::Error, Debug)]
pub enum RemoteStateError {
    #[error("access to state key '{key}' denied")]
    Permission { key: String },
    #[error("error accessing remote state: {message}")]
    Remote { message: String },
}

impl From<crate::bulwark_host::StateError> for RemoteStateError {
    fn from(error: crate::bulwark_host::StateError) -> Self {
        match error {
            crate::bulwark_host::StateError::Permission(key) => {
                RemoteStateError::Permission { key }
            }
            crate::bulwark_host::StateError::Remote(message) => {
                RemoteStateError::Remote { message }
            }
        }
    }
}

/// Returned when there is an issue with an http request sent by the plugin.
#[derive(thiserror::Error, Debug)]
pub enum HttpError {
    #[error("access to http host '{host}' denied")]
    Permission { host: String },
    #[error("invalid http method: '{method}'")]
    InvalidMethod { method: String },
    #[error("invalid uri: '{uri}'")]
    InvalidUri { uri: String },
    #[error("error sending http request: {message}")]
    Transmit { message: String },
    #[error("{message}")]
    UnavailableContent { message: String },
    #[error("{message}")]
    InvalidStart { message: String },
    #[error("{message}")]
    ContentTooLarge { message: String },
}

impl From<crate::bulwark_host::HttpError> for HttpError {
    fn from(error: crate::bulwark_host::HttpError) -> Self {
        match error {
            crate::bulwark_host::HttpError::Permission(host) => HttpError::Permission { host },
            crate::bulwark_host::HttpError::InvalidMethod(method) => {
                HttpError::InvalidMethod { method }
            }
            crate::bulwark_host::HttpError::InvalidUri(uri) => HttpError::InvalidUri { uri },
            crate::bulwark_host::HttpError::Transmit(message) => HttpError::Transmit { message },
            crate::bulwark_host::HttpError::UnavailableContent(message) => {
                HttpError::UnavailableContent { message }
            }
            crate::bulwark_host::HttpError::InvalidStart(message) => {
                HttpError::InvalidStart { message }
            }
            crate::bulwark_host::HttpError::ContentTooLarge(message) => {
                HttpError::ContentTooLarge { message }
            }
        }
    }
}
