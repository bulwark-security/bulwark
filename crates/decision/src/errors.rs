/// Returned when a threshold is either out-of-order or out-of-range.
#[derive(thiserror::Error, Debug)]
pub enum ThresholdError {
    #[error("invalid threshold order, must be trust < accept < suspicious < restrict")]
    ThresholdOutOfOrder,
    #[error("invalid threshold range, must be 0.0-1.0, got {0}")]
    ThresholdOutOfRange(f64),
}
