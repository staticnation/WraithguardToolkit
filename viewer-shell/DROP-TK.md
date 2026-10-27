# Dropping Tk and PyInstaller (plan)

Step 4.5 of the road in `TODO.md`: one Tauri build per platform, the Flatpak without its
Python runtime (and the `LD_LIBRARY_PATH` workaround), `native/` (pyo3) folded into
`viewcore` and `ordercore` as its last Python caller goes. It starts only when 4.1 to 4.4
have left no Python that a user runs.

Draft from `TODO.md` and the repo's file layout; I have not read the build scripts or the
Flatpak manifest in detail, so each slice starts by reading its files.

## Decided (Static, 2026-10-26)

- **The command line survives, in Rust**: the same flags as `wraithguard_toolkit.py`'s
  (`--cfg`, `--rules`, `--write-cfg`, `--emit-toml`, ... and `--check-conflicts`,
  `--resource-conflicts`, `--cell-map`, `--lint`), as a mode of the one binary over
  `ordercore`, `conflictcore` and `toolscore`, pinned by running both CLIs on the same inputs.
- **Translations keep the `.po` files as the source**; a generator builds the page's string
  tables from them, so the translators' work carries over.
- **Platforms: Windows, Linux (with the Flatpak), macOS, Android and iOS.** Tauri 2 builds
  all five. The mobile two are new ground and a slice of their own (10 below): no
  `std::process` spawning there (a second viewer window, tes3cmd and the OpenMW test run
  are desktop only), files come through the platform's pickers and app storage rather than
  free paths, and the page must work by touch.

## What goes

- The Tk app: `wraithguard_toolkit_gui.py` (5.8k lines), `wraithguard/gui/` (theme 1.9k,
  widgets 1.1k, the rest are the windows 4.1 to 4.4 retire), `wraithguard/viz/` (the Tk-side
  servers and HTML).
- Packaging for Python: `build/pyinstaller.txt`, `build/auto-py-to-exe_build.json`,
  `packaging/pyi_rth_pywebview_qt.py`, `build_linux.sh`, `build_linux.bat`,
  `Dockerfile.wraithguard`, the Python parts of `pyproject.toml` (413 lines).
- The Flatpak: `packaging/flatpak/` (manifest, `wraithguard.sh`, desktop and metainfo
  files). The manifest and metainfo stay; the Python runtime and the launcher shim go.
- `native/` (pyo3): its modules fold into the crates by what they are (`esp`, `uses`,
  `cache` already have editcore twins; `order` into ordercore; `land`, `merge`, `nif`, `img`,
  `bsa` into viewcore or the tool crates of 4.4).
- `wraithguard_toolkit.py` (6.3k lines, core and CLI).

## Slices

1. **The CLI in Rust** (decided: it survives). *Landed:* `viewer-shell/cli`, the
   `wraithguard` binary: every flag of `wraithguard_toolkit.py`, the sort and export
   (`ordercore::plan`), the record conflicts and their CSV, the cell map (`cli/src/cellmap.rs`,
   the page byte for byte), the lint, and the loose-file conflicts and their CSV. A console
   program of its own beside the viewer (the viewer is windowed on Windows, and this builds
   without a webview). Pinned by `tests/fixtures/cli_cases.json` (`tools/gen_cli_cases.py`: 15
   setups, the Python CLI's stdout, stderr, exit and files), run by `cli/tests/cli_cases.rs`.
   Accepted but unused: `--tes3conv`, `--json-dump-dir` (the reader is built in). English only
   until slice 3.
2. **Settings and data migration.** The Python app's settings file, caches, queue
   journal (version 1: the Rust side already reads it) and `.bak` files must open in the
   new app unchanged. Pin: a fixture folder from the last Python build opened by the new
   one. This is the step that must not lose a user's work.
   *Started:* `toolscore::settings` reads `wraithguard_toolkit_settings.json` (old keys and
   new) into the pages' fields (`settings_seed`): the Load order page opens on the rule
   files, TOML, list, cache, grass and options last sorted with (and sorts at once), the
   Tools page on tes3cmd and the Merge Lands folder, the Conflicts page on its exclusions;
   `theme` names the viewer's code for the Tk theme, for when `--theme` is gone. The Tk app
   now keeps keys it does not know when it saves, so the sort's fields survive it. Already
   carried: the journal (editcore reads version 1), the record cache (editcore's, no
   pickles), the `.bak` copies (the Tools page's Backups lists them). Left: the viewer wearing
   `theme` when its profile names none; the fixture folder test once the Tk app's last build
   is fixed.
3. **Translations** (`locale/`, `tools/build_locale.py`, `tools/make_pot.py`,
   `wraithguard/i18n.py`). Decided: the `.po` files stay the source; the page's string
   tables are generated from them, and its strings are extracted into the same `.pot`.
   *Deferred* (Static, 2026-10-27): English only until everything is in and working; the
   `.po` files are kept as they are for this slice later.
4a. **The program on its own and its Setup page.** *Landed:* started with no arguments
   (or only the viewer's flags) `wraithguard-viewer` is the program: the cell viewer on the
   settings file's openmw.cfg (`commands::settings_cfg`), the settings file beside it. The
   Setup page (`ui/src/64_wg_setup.js`; `setup_state`, `setup_check`, `settings_write`,
   `setup_find_cfgs`, `setup_mark`, `app_restart`; `toolscore::setup` and
   `toolscore::settings`) holds every path the pages run on, each checked as it is typed
   (a cfg's missing data folders, a TOML that does not parse, a rule file that is not there),
   and Find for OpenMW's own cfg locations. It opens by itself on a first start, after a start
   that never finished loading (the start is marked while it loads), and on `--setup`; the
   rest of the time it is the top bar's Setup button and a button on the loading card, over
   everything, with Restart for a load that will not finish. The Load order, Tools and
   Conflicts pages write their fields back to the same file. `ui/tests/boot.js` drives a first
   start (`WG_SETUP=1`).
4. **Fold `native/` into the crates**, one module at a time as the last Python caller goes
   (each module's own tests move with it). End state: `native/` and the pyo3 dependency are
   gone and `pyproject.toml` has nothing to build.
5. **One Tauri build per platform** in CI: Windows, Linux, macOS. The installer, icons, the
   file associations and the updater, if any. *Started:* `.github/workflows/build-app.yml`
   (sent as a file: the program, the command line and the documents, per platform, beside the
   PyInstaller builds until slice 8). Left: an installer, the WebView2-bundled Windows variant.
6. **The Flatpak** rebuilt on the Tauri binary: drop Python and `LD_LIBRARY_PATH`, keep the
   permissions the viewer needs (game folders, MO2 and OpenMW config paths). Check the
   `ubuntu-latest` move to Ubuntu 26 first (the TODO's watch item).
7. **A CSP.** `tauri.conf.json` has `"csp": null`; the page has about 230 inline `on...=`
   handlers (my count of the `ui` folder is higher, 774 lines match, so recount before
   scoping) that must move to `addEventListener`, page by page as each page is touched in
   4.1 to 4.4. Then set the policy, run the `WG_TAINT` runs once more, and mark Aikido's
   finding.
8. **Retire Python.** Delete `wraithguard/`, the root scripts, `tests/` Python files; keep
   `tests/fixtures/*_cases.json` as the crates' own tests (the 4.1 rule). Delete the
   generators in `tools/` only when no fixture needs regenerating; the ones that do stay as
   documentation of where the expected answers came from, or go with a note.
9. **After the last Python goes**, the dependency items: Tauri's pinned glib/gtk,
   string_cache and jsonptr are rechecked each Tauri release; CI actions move to the Node 24
   majors; Aikido closes what the Python removal makes moot.

10. **Android and iOS.** Tauri 2's mobile targets. First an audit of what the page and the
    commands assume of a desktop: spawned processes (`editor_preview_patch`, tes3cmd, the
    OpenMW test run), free file paths and `openmw.cfg` locations (on Android, OpenMW's own
    app keeps its files elsewhere), window management, hover-only controls, the gamepad and
    keyboard paths. Then the builds in CI and a device test of reading a real install.

## Where it stands (2026-10-27)

Done: 1 (the CLI), 2 (settings carried and written back; the theme and a fixture-folder test
left), 4a (the program on its own, the Setup page), 5 started (build-app.yml). Deferred: 3
(translations, English for now). Open: 4 and 8 (fold `native/`, retire the Python), which wait
for the gate below; 6 (the Flatpak on the Tauri binary); 7 (the CSP: the inline handlers);
9; 10 (Android and iOS).

## Gate

Do not delete the Python app until a user's real setup has run the new one end to end:
sort, export, edit, build a patch, merge lands, and open the old queue and settings.
