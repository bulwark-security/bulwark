use std::path::Path;

fn fetch_adapter() {
    let dest_path = Path::new("./adapter");
    let file_path = dest_path.join("wasi_snapshot_preview1.reactor.wasm");
    let body = reqwest::blocking::get("https://github.com/bytecodealliance/wasmtime/releases/download/dev/wasi_snapshot_preview1.reactor.wasm").unwrap().bytes().unwrap();
    std::fs::write(file_path, body).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    fetch_adapter();
}
