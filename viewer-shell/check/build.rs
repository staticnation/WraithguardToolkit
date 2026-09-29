use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("..").join("src").join("main.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("checked.rs");

    println!("cargo:rerun-if-changed={}", source.display());
    // Declare the cfg so rustc does not warn that it is unknown (unexpected_cfgs).
    println!("cargo:rustc-check-cfg=cfg(wg_check)");
    // Skip the cell-viewer module: it needs Tauri surface the stub doesn't model.
    println!("cargo:rustc-cfg=wg_check");

    match std::fs::read_to_string(&source) {
        Ok(text) => std::fs::write(&out, text).unwrap(),
        Err(error) => {
            println!(
                "cargo:warning=wraithguard-viewer-check: {} unreadable ({error})",
                source.display()
            );
            std::fs::write(&out, format!("compile_error!(\"viewer shell source unreadable: {error}\");\n")).unwrap();
        }
    }
}
