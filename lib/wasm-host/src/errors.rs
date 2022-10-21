use std::io;
use thiserror::Error;
use wasmer::{CompileError, InstantiationError};

#[derive(Error, Debug)]
pub enum PluginLoadError {
    /// An IO error
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("WebAssembly failed to compile: {0}")]
    Compile(#[from] CompileError),
    #[error("WebAssembly instance could not be created: {0}")]
    Instantiation(#[from] InstantiationError),
    #[error("Plugin was missing required parameter")]
    Build(),
}
