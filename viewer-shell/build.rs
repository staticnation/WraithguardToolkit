//! Assembles the cell viewer page (ui/ORDER -> ui-dist/index.html, with ui/LICENSE
//! as a comment after <head> and ui/viewer_only.html inserted before </head>), then
//! runs tauri-build, which embeds ui-dist as the app's frontend.

use std::path::Path;

fn build_page() {
    let ui = Path::new("ui");
    println!("cargo:rerun-if-changed=ui");
    let order = std::fs::read_to_string(ui.join("ORDER")).expect("ui/ORDER");
    let mut page = String::with_capacity(3 << 20);
    for name in order.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let part = std::fs::read_to_string(ui.join("src").join(name))
            .unwrap_or_else(|e| panic!("ui/src/{name}: {e}"));
        page.push_str(&part);
        if !part.ends_with('\n') {
            page.push('\n');
        }
    }
    // The page is GPL-2.0 (MGE XE shader ports): its licence and the provenance of every
    // part travel inside the page, so they ship with the viewer binary that embeds it.
    let licence = std::fs::read_to_string(ui.join("LICENSE")).expect("ui/LICENSE");
    println!("cargo:rerun-if-changed=ui/LICENSE");
    let head = page.find("<head>").expect("page has no <head>") + "<head>".len();
    page.insert_str(head, &format!("\n<!--\n{}\n-->\n", licence.replace("-->", "- ->")));
    let extra = std::fs::read_to_string(ui.join("viewer_only.html")).expect("ui/viewer_only.html");
    let at = page.find("</head>").expect("page has no </head>");
    page.insert_str(at, &extra);
    // Wraithguard's icon in the title bar, inlined so the page stays one file.
    let icon = std::fs::read(ui.join("wraithguard_icon.png")).expect("ui/wraithguard_icon.png");
    page = page.replace("__WG_ICON__", &format!("data:image/png;base64,{}", base64(&icon)));
    std::fs::create_dir_all("ui-dist").expect("ui-dist");
    std::fs::write("ui-dist/index.html", page).expect("ui-dist/index.html");
    // The page's own assets (GPL-2.0, see ui/LICENSE): copied beside it, so the viewer
    // embeds them with the page and the page can fetch them by name. MGE XE's wave volume
    // is the one that matters: an OpenMW install does not have it.
    println!("cargo:rerun-if-changed=ui/assets");
    if let Ok(dir) = std::fs::read_dir(ui.join("assets")) {
        for entry in dir.flatten() {
            let from = entry.path();
            if from.is_file() {
                let to = Path::new("ui-dist").join(entry.file_name());
                std::fs::copy(&from, &to).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
            }
        }
    }
}

/// tauri-build needs the window icon at compile time. The one copy lives at the repo
/// root (see icons/README.md); bring it in when it is not already here, so a plain
/// `cargo build` works the same locally as in CI.
fn copy_icon() {
    let want = Path::new("icons/wraithguard_toolkit_icon.ico");
    let root = Path::new("../wraithguard_toolkit_icon.ico");
    println!("cargo:rerun-if-changed=../wraithguard_toolkit_icon.ico");
    if root.is_file() && !want.is_file() {
        std::fs::copy(root, want).expect("copy the window icon into viewer-shell/icons");
    }
}

/// Standard base64, for inlining the icon (a build dependency for twenty lines is not worth it).
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn main() {
    copy_icon();
    build_page();
    tauri_build::build()
}
