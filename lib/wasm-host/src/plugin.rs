use crate::PluginLoadError;
// use bulwark_wasm_sdk::types::{Decision, Status};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

// Owns a single detection plugin and provides the interface between WASM host and guest.
pub struct Plugin {}

impl Plugin {
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, PluginLoadError> {
        Ok(Plugin {})
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PluginLoadError> {
        Ok(Plugin {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO:
    // include_bytes!
    // cargo build --package bulwark-wasm-sdk --lib --verbose --target wasm32-unknown-unknown --example evil_param

    #[test]
    fn test_wasm_execution() -> Result<(), Box<dyn std::error::Error>> {
        // let wasm_bytes = include_bytes!("../test/dont_know.wasm");
        // let mut plugin = Plugin::from_bytes(wasm_bytes)?;

        Ok(())
    }
}
