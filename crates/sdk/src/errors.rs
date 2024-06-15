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
