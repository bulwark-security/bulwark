use crate::errors::BuildError;
use cargo_metadata::Message;
use cargo_metadata::{CargoOpt, MetadataCommand};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn plugin_name(path: impl AsRef<Path>) -> Result<String, BuildError> {
    let path = path.as_ref();

    let metadata = MetadataCommand::new()
        .manifest_path(path.join("Cargo.toml"))
        .exec()?;

    let root = metadata.root_package().ok_or(BuildError::MissingMetadata)?;
    Ok(root.name.clone())
}

pub fn wasm_filename(path: impl AsRef<Path>) -> Result<String, BuildError> {
    let plugin_name = plugin_name(path)?;
    Ok(format!("{}.wasm", plugin_name.replace("-", "_")))
}

fn adapt_wasm_output(wasm_bytes: Vec<u8>, adapter_bytes: Vec<u8>) -> Result<Vec<u8>, BuildError> {
    let component = wit_component::ComponentEncoder::default()
        .module(&wasm_bytes)
        .map_err(|err| BuildError::Adapter(err.to_string()))?
        .validate(true)
        .adapter("wasi_snapshot_preview1", &adapter_bytes)
        .map_err(|err| BuildError::Adapter(err.to_string()))?
        .encode()
        .map_err(|err| BuildError::Adapter(err.to_string()))?;

    Ok(component.to_vec())
}

/// Builds a plugin.
///
/// Compiles the plugin with the `wasm32-wasi` target, and installs it if it is missing.
/// Uses an embeded adapter WASM file to adapt from preview 1 to preview 2 for the component
/// model.
///
/// Calls out to `cargo` via [`Command`], so `cargo` must be available on the path for this
/// function to work.
pub fn build_plugin(path: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<(), BuildError> {
    // TODO: install wasm32-wasi target if missing
    let adapter_bytes = include_bytes!("../adapter/wasi_snapshot_preview1.reactor.wasm");
    let path = path.as_ref();
    let output = output.as_ref();
    let output_dir = output.parent().ok_or(BuildError::MissingParent)?;

    let mut args = vec!["build", "--target=wasm32-wasi"];
    // TODO: don't hard-code --release
    let release = true;
    if release {
        args.push("--release");
    }

    let mut command = Command::new("cargo").args(&args).spawn()?;

    let exit_status = command.wait()?;
    if exit_status.success() {
        let wasm_filename = wasm_filename(path)?;
        let wasm_path = path
            .join("target/wasm32-wasi")
            .join(if release { "release" } else { "debug" })
            .join(wasm_filename);
        let wasm_bytes = std::fs::read(&wasm_path)
            .map_err(|err| BuildError::NotFound(wasm_path.to_string_lossy().to_string(), err))?;

        let adapted_bytes = adapt_wasm_output(wasm_bytes, adapter_bytes.to_vec())?;
        std::fs::create_dir_all(output_dir)?;
        std::fs::write(output, adapted_bytes)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_name() -> Result<(), Box<dyn std::error::Error>> {
        let bulwark_plugin_name = plugin_name(std::env::current_dir()?)?;
        assert_eq!(bulwark_plugin_name, "bulwark-cli");
        Ok(())
    }

    #[test]
    fn test_wasm_filename() -> Result<(), Box<dyn std::error::Error>> {
        // Doesn't matter that this particular crate isn't a plugin, if it works here it'll work for a real plugin.
        let wasm_filename = wasm_filename(std::env::current_dir()?)?;
        assert_eq!(wasm_filename, "bulwark_cli.wasm");
        Ok(())
    }
}
