mod errors;

pub use crate::errors::*;

use cargo_metadata::MetadataCommand;
use std::collections::HashMap;
use std::io::prelude::*;
use std::path::Path;
use std::process::{Command, Stdio};

/// Returns the name of the plugin as read from the Cargo metadata.
fn plugin_name(path: impl AsRef<Path>) -> Result<String, BuildError> {
    let path = path.as_ref();

    let metadata = MetadataCommand::new()
        .manifest_path(path.join("Cargo.toml"))
        .exec()?;

    let root = metadata.root_package().ok_or(BuildError::MissingMetadata)?;
    Ok(root.name.clone())
}

/// Returns the filename that the compiled plugin will use.
pub fn wasm_filename(path: impl AsRef<Path>) -> Result<String, BuildError> {
    let plugin_name = plugin_name(path)?;
    Ok(format!("{}.wasm", plugin_name.replace('-', "_")))
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

fn installed_targets() -> Result<HashMap<String, bool>, BuildError> {
    let mut command = Command::new("rustup")
        .args(["target", "list"])
        .stdout(Stdio::piped())
        .spawn()?;

    let reader = std::io::BufReader::new(command.stdout.take().unwrap());
    let mut targets = HashMap::new();
    for line in reader.lines() {
        let line = line?;
        let installed = line.contains("(installed)");
        let target = line.replace("(installed)", "").trim().to_string();
        targets.insert(target, installed);
    }
    let exit_status = command.wait()?;
    if !exit_status.success() {
        return Err(BuildError::SubprocessError);
    }
    Ok(targets)
}

fn install_wasm32_wasi_target() -> Result<(), BuildError> {
    let mut command = Command::new("rustup")
        .args(["target", "install", "wasm32-wasi"])
        .spawn()?;
    let exit_status = command.wait()?;
    if !exit_status.success() {
        return Err(BuildError::SubprocessError);
    }
    Ok(())
}

/// Builds a plugin.
///
/// Compiles the plugin with the `wasm32-wasi` target, and prompts to install it if it is missing.
/// Uses an embeded adapter WASM file to adapt from preview 1 to preview 2 for the component
/// model.
///
/// Calls out to `cargo` via [`Command`], so `cargo` must be available on the path for this
/// function to work.
pub fn interactive_build_plugin(
    path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    additional_args: &[String],
) -> Result<(), BuildError> {
    let mut install_missing = false;
    let installed_targets = installed_targets()?;
    let wasi_installed = installed_targets.get("wasm32-wasi");
    if !wasi_installed.unwrap_or(&false) {
        println!("The required wasm32-wasi target is not installed.");
        print!("Install it? (y/N) ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_ascii_lowercase();
        if &input == "y" || &input == "yes" {
            install_missing = true;
        } else {
            return Err(BuildError::MissingTarget);
        }
    }
    build_plugin(path, output, additional_args, install_missing)
}

/// Builds a plugin.
///
/// Compiles the plugin with the `wasm32-wasi` target, and installs it if it is missing.
/// Uses an embeded adapter WASM file to adapt from preview 1 to preview 2 for the component
/// model.
///
/// Calls out to `cargo` via [`Command`], so `cargo` must be available on the path for this
/// function to work.
pub fn build_plugin(
    path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    additional_args: &[String],
    install_missing: bool,
) -> Result<(), BuildError> {
    // TODO: install wasm32-wasi target if missing
    let adapter_bytes = include_bytes!("../../../adapter/wasi_snapshot_preview1.reactor.wasm");
    let path = path.as_ref();
    let output = output.as_ref();
    let output_dir = output.parent().ok_or(BuildError::MissingParent)?;

    let installed_targets = installed_targets()?;
    let wasi_installed = installed_targets.get("wasm32-wasi");
    if !wasi_installed.unwrap_or(&false) {
        if install_missing {
            install_wasm32_wasi_target()?;
        } else {
            return Err(BuildError::MissingTarget);
        }
    }

    let mut args = vec!["build", "--target=wasm32-wasi", "--release"];
    let mut additional_args = additional_args.iter().map(|arg| arg.as_str()).collect();
    args.append(&mut additional_args);

    let mut command = Command::new("cargo")
        .current_dir(path)
        .args(&args)
        .spawn()?;

    let exit_status = command.wait()?;
    if exit_status.success() {
        let wasm_filename = wasm_filename(path)?;
        let wasm_path = path.join("target/wasm32-wasi/release").join(wasm_filename);
        let wasm_bytes = std::fs::read(&wasm_path)
            .map_err(|err| BuildError::NotFound(wasm_path.to_string_lossy().to_string(), err))?;

        let adapted_bytes = adapt_wasm_output(wasm_bytes, adapter_bytes.to_vec())?;
        std::fs::create_dir_all(output_dir)?;
        std::fs::write(output, adapted_bytes)?;
    } else {
        return Err(BuildError::SubprocessError);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_name() -> Result<(), Box<dyn std::error::Error>> {
        let bulwark_plugin_name = plugin_name(std::env::current_dir()?)?;
        assert_eq!(bulwark_plugin_name, "bulwark-build");
        Ok(())
    }

    #[test]
    fn test_wasm_filename() -> Result<(), Box<dyn std::error::Error>> {
        // Doesn't matter that this particular crate isn't a plugin, if it works here it'll work for a real plugin.
        let wasm_filename = wasm_filename(std::env::current_dir()?)?;
        assert_eq!(wasm_filename, "bulwark_build.wasm");
        Ok(())
    }
}
