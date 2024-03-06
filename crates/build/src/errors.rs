#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("missing file '{0}': {1}")]
    NotFound(String, std::io::Error),
    #[error("missing parent directory")]
    MissingParent,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("subprocess had non-zero exit status")]
    SubprocessError,
    #[error("error reading plugin metadata: {0}")]
    CargoMetadata(#[from] cargo_metadata::Error),
    #[error("missing plugin metadata")]
    MissingMetadata,
    #[error("missing required wasm32-wasi target")]
    MissingTarget,
    #[error("error adapting wasm: {0}")]
    Adapter(String),
}
