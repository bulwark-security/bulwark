use thiserror::Error;
use wasmer::{CompileError, InstantiationError};

#[derive(Error, Debug)]
pub enum ComponentLoadError {
    #[error("WebAssembly failed to compile: {0}")]
    Compile(CompileError),
    #[error("WebAssembly instance could not be created: {0}")]
    Instantiation(InstantiationError),
    #[error("Component was missing required parameter")]
    Build(),
}
