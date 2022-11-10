use std::process::Command;

fn main() {
    // cargo install --git https://github.com/bytecodealliance/wit-bindgen --rev e7fd381c8bf6454c1ed638f2cd0c55ab1c81f81f wit-bindgen-cli
    Command::new("wit-bindgen")
        .args(&[
            "guest",
            "rust",
            "--export",
            "../../interface.wit",
            "--out-dir",
            "src",
            "--name",
            "bindings",
        ])
        .status()
        .unwrap();

    // Tell Cargo to rebuild bindings if the WIT file changes
    println!("cargo:rerun-if-changed=../../interface.wit");
}
