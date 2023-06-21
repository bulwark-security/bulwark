use std::fs;
use std::path::Path;
use std::process::Command;

fn fetch_adapter() {
    let dest_path = Path::new("./tests");
    let file_path = dest_path.join("wasi_snapshot_preview1.reactor.wasm");
    let body = reqwest::blocking::get("https://github.com/bytecodealliance/wasmtime/releases/download/dev/wasi_snapshot_preview1.reactor.wasm").unwrap().bytes().unwrap();
    std::fs::write(file_path, body).unwrap();
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
    println!("cargo:rerun-if-changed=build.rs");
    build_test_example_wasm("blank-slate");
    build_test_example_wasm("evil-bit");
    fetch_adapter();
}
