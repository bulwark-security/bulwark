use std::io;
use thiserror::Error;
use wasmer::{CompileError, ExportError, InstantiationError, RuntimeError};

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

#[derive(Error, Debug)]
pub enum AllocationError {
    #[error("WebAssembly failed to export: {0}")]
    Compile(#[from] ExportError),
    #[error("WebAssembly failed to run: {0}")]
    Runtime(#[from] RuntimeError),
}
