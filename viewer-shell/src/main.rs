// wraithguard-viewer: a single-purpose native webview window.
//
// This is the entire replacement for pywebview in WraithguardToolkit. It is
// invoked exactly the way the old pywebview child process was invoked --
// spawned by _open_cell_map_pywebview() in wraithguard_toolkit_gui.py -- but
// as a standalone binary, not a re-invocation of the Python interpreter. It
// takes a URL and a window title (and optionally a width/height) on argv,
// opens one window pointed at that URL, and exits when the window closes.
//
// Deliberately minimal: no bundled frontend, no IPC commands, no plugins.
// The content it displays is always served by the Python app over loopback
// (see _serve_html_file() on the Python side) -- this binary is just the
// native chrome around that page. Because it never touches a Python
// interpreter or a Python C-extension binding, it carries none of
// PyQt6/sip's free-threading limitations: the Python process can run
// GIL-free (3.14t) or not, and it makes no difference to this binary.
//
// Usage: wraithguard-viewer <url> [title] [width] [height]
//        wraithguard-viewer --cell-viewer [--openmw-cfg <path>] [--cell x,y|int:Name] [--title <t>]
//          (the built-in cell viewer; see src/cellviewer)
//        wraithguard-viewer [--setup] [--loadorder ...]
//          (the program on its own: the cell viewer on the settings file's openmw.cfg, its
//          Setup page first when there is none yet or `--setup` asks; port step 4.5)

use tauri::{WebviewUrl, WebviewWindowBuilder};

// The stub compile-check crate (check/) models only the URL mode; the command
// bodies are compiled separately by check-commands/. The real build compiles both.
#[cfg(not(wg_check))]
mod cellviewer;

/// What the Python launcher (`wraithguard/viewer_launch.py`) set in the viewer's
/// environment before 5.0, set by the program itself now that it starts on its own; a
/// value already in the environment is left as it is.
///
/// - Windows, the WebView2-bundled build: the Fixed Version runtime in a `webview2` folder
///   beside the program (as Microsoft ships it, one versioned folder inside), for machines
///   without the system runtime (WebView2 removed with Edge, LTSC, stripped images).
/// - Linux: WebKitGTK's DMA-BUF renderer off; it fails on several GPU stacks, the Deck's
///   included.
fn launch_env() {
    #[cfg(windows)]
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_none() {
        const EXE: &str = "msedgewebview2.exe";
        let base = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("webview2")));
        let found = base.and_then(|base| {
            if base.join(EXE).is_file() {
                return Some(base);
            }
            let mut subs: Vec<std::path::PathBuf> = std::fs::read_dir(&base).ok()?.flatten().map(|e| e.path()).filter(|p| p.join(EXE).is_file()).collect();
            subs.sort();
            subs.pop()
        });
        if let Some(dir) = found {
            // The first thing main does, before any thread is started.
            std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", dir);
        }
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

fn main() {
    launch_env();
    let args: Vec<String> = std::env::args().collect();

    // One context for the whole binary. On macOS `generate_context!` embeds the
    // Info.plist as a named static (`_EMBED_INFO_PLIST`), so expanding it in both
    // modes -- here and in the cell viewer -- fails to link with a duplicate symbol.
    let context = tauri::generate_context!();

    #[cfg(not(wg_check))]
    if args.get(1).map(String::as_str) == Some("--cell-viewer") {
        std::process::exit(cellviewer::run(&args[2..], context));
    }
    // Port step 4.5: started on its own - no arguments, or only the cell viewer's flags
    // (`--setup` ...) - it is the program: the cell viewer on the settings file's openmw.cfg,
    // or on its Setup page when there is none yet.
    #[cfg(not(wg_check))]
    if args.len() == 1 || args.get(1).is_some_and(|a| a.starts_with("--")) {
        std::process::exit(cellviewer::run(&args[1..], context));
    }

    let url_arg = match args.get(1) {
        Some(u) => u.clone(),
        None => {
            eprintln!("wraithguard-viewer: missing required <url> argument");
            std::process::exit(2);
        }
    };
    let title = args.get(2).cloned().unwrap_or_else(|| "Viewer".to_string());
    let width: f64 = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1050.0);
    let height: f64 = args
        .get(4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(760.0);

    let parsed_url = match url_arg.parse() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("wraithguard-viewer: invalid URL '{url_arg}': {e}");
            std::process::exit(2);
        }
    };

    let result = tauri::Builder::default()
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "viewer", WebviewUrl::External(parsed_url))
                .title(&title)
                .inner_size(width, height)
                // The main Tkinter app owns focus/taskbar semantics for the
                // rest of the toolkit; this window should behave like a
                // normal top-level window, not steal activation tricks.
                .visible(true)
                .build()?;
            Ok(())
        })
        // No window found after setup (e.g. platform webview init failed) is
        // the same failure shape pywebview could hit -- exit non-zero so the
        // Python side's existing browser fallback (see open_html_in_app) can
        // catch it via the subprocess return code, same as before.
        .run(context);

    if let Err(e) = result {
        eprintln!("wraithguard-viewer: fatal error: {e}");
        std::process::exit(1);
    }
}
