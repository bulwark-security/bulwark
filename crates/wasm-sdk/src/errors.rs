/// Generic result
pub type Result = ::std::result::Result<(), Error>;

/// Generic error
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ParseCounter(#[from] ParseCounterError),
    /// Triggered when there can only be a single validation error
    #[error(transparent)]
    Validation(#[from] validator::ValidationError),
    /// Triggered when there could be multiple validation errors
    #[error(transparent)]
    Validations(#[from] validator::ValidationErrors),
    /// Catch-all error type
    #[error(transparent)]
    AnyError(#[from] anyhow::Error),
}

/// Returned when an attempt to parse a counter within a plugin environment fails.
#[derive(thiserror::Error, Debug)]
pub enum ParseCounterError {
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
}
