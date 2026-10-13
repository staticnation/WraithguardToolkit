//! Writes the oracle tests' meshes for the C++ (`tools/meshopt_oracle`):
//!
//!   cargo run --example dump_parts -- <out.bin> [<folder of .nif files>]
//!
//! With a folder, its NIFs (recursively) instead of the test set: the corpus run.

#[path = "../tests/common/mod.rs"]
mod common;

fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("nif")) {
            out.push(p);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).expect("usage: dump_parts <out.bin> [<nif folder>]");
    let parts = match args.get(2) {
        None => common::all_parts(),
        Some(dir) => {
            let root = std::path::Path::new(dir);
            let mut files = Vec::new();
            walk(root, &mut files);
            files.sort();
            let mut parts = Vec::new();
            for f in files {
                let rel = f.strip_prefix(root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                if let Ok(b) = std::fs::read(&f) {
                    parts.extend(common::nif_parts(&b, &rel));
                }
            }
            parts
        }
    };
    std::fs::write(out, common::dump(&parts)).unwrap();
    eprintln!("{} parts", parts.len());
}
