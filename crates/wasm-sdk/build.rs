use std::path::Path;
use std::process::Command;

#[allow(dead_code)]
fn build_wit_bindings() {
    let dest_path = Path::new("./src");
    let wit_path = Path::new("../../bulwark-host.wit");

    let status = Command::new("wit-bindgen")
        .arg("rust-wasm")
        .arg("--import")
        .arg(wit_path)
        .arg("--out-dir")
        .arg(dest_path)
        .status()
        .unwrap();
    if !status.success() {
        panic!("codegen failed");
    }

    println!("cargo:rerun-if-changed={}", wit_path.display());
}

fn main() {
    // Not needed when using import/export macros
    // build_wit_bindings();
}
