use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("..").join("src").join("main.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("checked.rs");

    println!("cargo:rerun-if-changed={}", source.display());

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
