use std::fs;
use std::path::Path;
use std::process::Command;

#[allow(dead_code)]
fn build_wit_bindings() {
    let dest_path = Path::new("./src");
    let wit_path = Path::new("../../bulwark-host.wit");

    let status = Command::new("wit-bindgen")
        .arg("wasmtime")
        .arg("--export")
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

#[allow(unused_must_use)]
fn build_test_example_wasm(name: &str) {
    let dest_path = Path::new("./tests");
    let sdk_path = Path::new("../wasm-sdk");
    let binding = sdk_path.join("examples").join(name);
    let example_path = binding.as_path();
    fs::create_dir(dest_path);

    // ensure we don't copy in old builds
    Command::new("cargo")
        .args(["clean", "--target", "wasm32-wasi", "--release"])
        .current_dir(example_path)
        .status()
        .unwrap();

    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-wasi", "--release"])
        .current_dir(example_path)
        .status()
        .unwrap();
    if !status.success() {
        panic!("example build failed");
    }

    println!(
        "cargo:rerun-if-changed=../wasm-sdk/examples/{}/src/main.rs",
        name
    );
    println!(
        "cargo:rerun-if-changed=../wasm-sdk/examples/{}/Cargo.toml",
        name
    );
    println!("cargo:rerun-if-changed=../wasm-sdk/src/host_calls.rs");
    println!("cargo:rerun-if-changed=../wasm-sdk/src/lib.rs");
}

fn main() {
    // Not needed when using import/export macros
    // build_wit_bindings();
    build_test_example_wasm("blank-slate");
    build_test_example_wasm("evil-bit");
}
