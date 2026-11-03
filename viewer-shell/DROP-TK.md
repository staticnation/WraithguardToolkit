# After the port: what is left of dropping Tk and Python

Since 5.0 (2026-10-27) Wraithguard is one Rust program: the viewer with its pages, and the
`wraithguard` command line beside it. The Python app, `native/` and the PyInstaller packaging
are gone (Static ran the new one end to end on a real setup first: sort, export, edit, build a
patch, merge lands, Save Check). How it got there is in `CHANGELOG.md` (4.x and 5.0.0). This
file is what is left, with what was decided for each.

## Decided (Static, 2026-10-26)

- **The command line stays**, in Rust (`viewer-shell/cli`): the flags of the old
  `wraithguard_toolkit.py`, pinned by `tests/fixtures/cli_cases.json`.
- **Translations keep the `.po` files as the source**; the page's string tables are generated
  from them, and its strings extracted into the same `.pot`.
- **Platforms: Windows, Linux (with the Flatpak), macOS, Android and iOS.**

## Left

1. **Translations.** English only until everything is in and working (Static, 2026-10-27);
   the `.po` files in `locale/` are kept as they are for this.
2. **The Flatpak on the Tauri binary.** `build-flatpak.yml` still packages 4.x and fails
   until it is rebuilt: drop the Python runtime and the `LD_LIBRARY_PATH` shim, keep the
   permissions the viewer needs (game folders, MO2 and OpenMW config paths). Check the
   `ubuntu-latest` move to Ubuntu 26 first.
3. **A CSP.** `tauri.conf.json` has `"csp": null`. The page's inline `on...=` handlers move to
   `addEventListener` first (recount them before scoping), then the policy is set, the
   `WG_TAINT` runs go once more, and Aikido's finding is marked.
4. **An installer**, and a WebView2-bundled Windows variant (`build-app.yml` builds the
   program, the command line and the documents per platform).
5. **The theme from the Tk settings**: `toolscore::settings` reads `theme`; the viewer should
   wear it when its profile names none.
6. **Lint on the scan cache.**
7. **The dependency items**, each Tauri release: its pinned glib/gtk, string_cache and
   jsonptr rechecked; CI actions on the Node 24 majors; Aikido's findings that the Python
   removal made moot closed.
8. **Android and iOS** (Tauri 2's mobile targets). First an audit of what the page and the
   commands assume of a desktop: spawned processes (`editor_preview_patch`, tes3cmd, the
   OpenMW test run), free file paths and `openmw.cfg` locations (OpenMW's Android app keeps
   its files elsewhere), window management, hover-only controls, the gamepad and keyboard
   paths. Then the builds in CI and a device test reading a real install.
