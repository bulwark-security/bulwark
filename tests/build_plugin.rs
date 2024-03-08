use std::path::Path;

#[test]
fn test_example_builds() -> Result<(), Box<dyn std::error::Error>> {
    let base = Path::new(file!()).parent().unwrap_or(Path::new("."));

    bulwark_build::build_plugin(
        base.join("../crates/wasm-sdk/examples/blank-slate"),
        base.join("dist/plugins/bulwark_blank_slate.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/bulwark_blank_slate.wasm").exists());

    bulwark_build::build_plugin(
        base.join("../crates/wasm-sdk/examples/evil-bit"),
        base.join("dist/plugins/bulwark_evil_bit.wasm"),
        &[],
        true,
    )?;
    assert!(base.join("dist/plugins/bulwark_evil_bit.wasm").exists());

    Ok(())
}
