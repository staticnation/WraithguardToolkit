//! Rewrites `viewer-shell/src/cellviewer/commands.rs` into something this crate can compile.
//!
//! The real Tauri crate needs its whole dependency tree from crates.io, so it is not
//! built everywhere its callers are edited: a signature change in `gdn_core` can break
//! the shell silently and only surface on the machine that runs `cargo tauri dev`.
//! This crate is that missing compiler pass, and this file is what keeps it honest.
//!
//! It used to be a Python script writing a tracked `src/lib.rs`, and that had two
//! failure modes that both bit. A machine without Python skipped the check entirely —
//! `build.cmd` says so out loud — and the generated file, being tracked, could be left
//! behind when `main.rs` moved, which turns into an error inside a file the reader was
//! told not to edit, about a field that no longer exists. Generating into `OUT_DIR` at
//! build time cannot go stale, needs nothing installed, and leaves nothing to forget.
//!
//! What is stripped: the entry point (all Tauri and nothing else), the command
//! attribute, and the windowing `cfg_attr`. What is kept: the command bodies, which
//! are the part that actually calls into the engine — published, so that `gardenfell
//! serve` can run them for the browser tests rather than a second copy being written.

use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("..").join("src").join("cellviewer").join("commands.rs");
    let stub = manifest.join("src").join("stub.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("checked.rs");

    // Rebuild whenever either input moves. Without this the check silently freezes at
    // whatever main.rs said the first time, which is the bug this file exists to end.
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}", stub.display());

    let text = match std::fs::read_to_string(&src) {
        Ok(t) => t,
        Err(e) => {
            // Not fatal: a checkout without the shell should still build the engine.
            println!("cargo:warning=wraithguard-viewer-check-commands: {} unreadable ({e}); nothing checked", src.display());
            std::fs::write(&out, "// viewer-shell/src/cellviewer/commands.rs was not readable\n").unwrap();
            return;
        }
    };

    match rewrite(&text, &stub) {
        Ok(body) => std::fs::write(&out, body).unwrap(),
        Err(why) => {
            // A restructured main.rs should say so plainly rather than produce a file
            // that fails to compile for reasons that have nothing to do with the edit.
            println!("cargo:warning=wraithguard-viewer-check-commands: {why}; nothing checked");
            std::fs::write(&out, format!("// not generated: {why}\n")).unwrap();
        }
    }
}

fn rewrite(text: &str, stub: &Path) -> Result<String, String> {
    let cut = text
        .find("\npub fn builder()")
        .ok_or("no `fn main()` in viewer-shell/src/cellviewer/commands.rs — has the file been restructured?")?;

    let mut out = String::with_capacity(cut + 512);
    /* Line for line with main.rs, deliberately. Nothing is deleted and nothing is
       inserted on a line of its own — removals become blank lines and the stub goes on
       the end of an existing one — so a compile error rustc reports against
       `checked.rs:754` is at `main.rs:754`, which is the file the reader has open. The
       old Python generator shifted everything by thirty-odd lines and left you counting.

       `include!` splices this into the middle of lib.rs, so an inner attribute or an
       inner doc comment here would be a hard error. Both are demoted rather than
       dropped: the text belongs next to the code it describes, and someone who lands
       in OUT_DIR should still be able to tell what they are looking at. */
    /* Commands that would run on the main thread, which none of them may.
     *
     * Tauri runs a command without the `async` marker on the main thread, and on Windows
     * the WebView2 window is on that thread — so a plain `#[tauri::command]` doing real
     * work freezes the application for as long as it takes. It is the kind of mistake
     * nothing else here can catch: the browser suites drive the engine as a separate
     * process behind a pipe, where the page waits rather than blocks, so they would stay
     * green while the desktop build locked up. Reported by line number, because that is
     * the one thing the reader needs. */
    let mut bare: Vec<usize> = Vec::new();
    /* Every function a command attribute actually landed on, in order — which is not the
       same as every function somebody meant it to land on. See `handler_gaps`. */
    let mut commands: Vec<String> = Vec::new();
    let mut want_command = false;

    for (no, line) in text[..cut].lines().enumerate() {
        let t = line.trim_start();
        if t == "#[tauri::command]" {
            bare.push(no + 1);
        }
        if t == "#[tauri::command]" || t == "#[tauri::command(async)]" {
            want_command = true;
        } else if want_command && line.starts_with("fn ") {
            // The function the attribute actually landed on, whatever its author meant.
            let name = line["fn ".len()..]
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                commands.push(name.to_string());
            }
            want_command = false;
        }
        /* `serde` is the shell's dependency, not this crate's: the derive exists so
           Tauri can turn a webview payload into the struct, and nothing here ever
           deserialises anything. Dropped like the command attribute, on the same terms
           — a blank line, so the line numbering still matches. */
        if t.starts_with("#![")
            || t == "#[tauri::command]"
            || t == "#[tauri::command(async)]"
            || t == "#[derive(serde::Deserialize)]"
        {
            out.push('\n');
            continue;
        }
        if let Some(rest) = t.strip_prefix("//!") {
            out.push_str("//");
            out.push_str(rest);
        } else if let Some((kw, rest)) = ["fn ", "struct ", "type "]
            .iter()
            .find_map(|kw| line.strip_prefix(kw).map(|r| (*kw, r)))
        {
            /* Published, so the commands can be *called* and not merely compiled. That
               is what lets `GardenfellCLI serve` run the real command bodies without
               Tauri, which is what the browser tests drive. A prefix, and it saves a
               second copy of every command being written out for testing.

               Top level only — the `line` here still has its indentation, so a nested
               item is left alone. */
            out.push_str("pub ");
            out.push_str(kw);
            out.push_str(rest);
        } else {
            out.push_str(line);
            // The stub stands in for the `tauri` crate. An absolute path because a
            // relative one would be resolved against OUT_DIR.
            if t == "use std::sync::Mutex;" {
                out.push_str(&format!(
                    " #[path = {:?}] pub mod tauri;",
                    stub.to_string_lossy().replace('\\', "/")
                ));
            }
        }
        out.push('\n');
    }
    if !out.contains("mod tauri;") {
        return Err("could not find `use std::sync::Mutex;` to hang the Tauri stub on".into());
    }
    /* And the other way the shell breaks without anything here noticing: a name in
       `generate_handler!` that is not a command.
     *
       It happened by writing a small helper directly under a command's attribute, which
       silently moved that attribute onto the helper and left the command undecorated.
       Nothing in this crate could see it — the attribute is stripped here either way —
       and the first machine to find out was Robin's, with `cannot find macro
       __cmd__interior_data in this scope` against a function that is plainly there.
       So: every name the handler list mentions must be a function this file decorated. */
    let missing = handler_gaps(text, &commands);
    if !missing.is_empty() {
        out.push_str(&format!(
            "compile_error!(\"viewer-shell/src/cellviewer/commands.rs: generate_handler! names {} which \
             {} not #[tauri::command] function(s). A helper written between a command's \
             attribute and its `fn` takes the attribute with it — check what sits above \
             each of those functions.\");\n",
            missing.join(", "),
            if missing.len() == 1 { "is" } else { "are" },
        ));
    }
    if !bare.is_empty() {
        /* A `compile_error!` rather than a `cargo:warning`: a warning about a frozen
           window scrolls past with the rest of the build. Appended after the stripped
           body, so the line numbers above it still match `main.rs`. */
        out.push_str(&format!(
            "compile_error!(\"viewer-shell/src/cellviewer/commands.rs: #[tauri::command] on line(s) {} would \
             run on the main thread, which is the thread the window is drawn on. Use \
             #[tauri::command(async)] — see the note at the top of that file.\");\n",
            bare.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(out)
}

/// The names `generate_handler!` lists that no `#[tauri::command]` function answers to.
///
/// The handler list lives after `fn main()`, which the rewrite above cuts away, so this
/// reads the original text. Anything that is not a bare identifier — a comment, a stray
/// bracket — is ignored rather than guessed at: this is a check, and a check that
/// invents work for the reader is worse than none.
fn handler_gaps(text: &str, commands: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(at) = text.find("generate_handler![") else { return out };
    let rest = &text[at + "generate_handler![".len()..];
    let Some(end) = rest.find(']') else { return out };
    for raw in rest[..end].split(',') {
        // One name per entry, with the comments a list like this collects stripped off.
        let name: String = raw
            .lines()
            .map(|l| l.split("//").next().unwrap_or("").trim())
            .collect::<Vec<_>>()
            .join("");
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        if !commands.iter().any(|c| c == name) && !out.iter().any(|o| o == name) {
            out.push(name.to_string());
        }
    }
    out
}
