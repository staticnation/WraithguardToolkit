# Dropping Tk and PyInstaller (plan)

Step 4.5 of the road in `TODO.md`: one Tauri build per platform, the Flatpak without its
Python runtime (and the `LD_LIBRARY_PATH` workaround), `native/` (pyo3) folded into
`viewcore` and `ordercore` as its last Python caller goes. It starts only when 4.1 to 4.4
have left no Python that a user runs.

Draft from `TODO.md` and the repo's file layout; I have not read the build scripts or the
Flatpak manifest in detail, so each slice starts by reading its files.

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

1. **Decide the CLI.** `wraithguard_toolkit.py` is both the library and a command line.
   Decide: does the command line survive (a `wraithguard` binary over the same crates) or
   go with the window? Everything after depends on it. If it survives, it is a slice of
   its own: the commands over `ordercore`/`editcore`/the tool crates, pinned by running the
   Python CLI and the Rust one on the same inputs.
2. **Settings and data migration.** The Python app's settings file, caches, queue
   journal (version 1: the Rust side already reads it) and `.bak` files must open in the
   new app unchanged. Pin: a fixture folder from the last Python build opened by the new
   one. This is the step that must not lose a user's work.
3. **Translations** (`locale/`, `tools/build_locale.py`, `tools/make_pot.py`,
   `wraithguard/i18n.py`). The Python app uses gettext `_()`; the page needs its own
   mechanism. Decide: keep the .po files as the source and generate the page's tables from
   them, or move to something else; either way the translators' files must survive.
4. **Fold `native/` into the crates**, one module at a time as the last Python caller goes
   (each module's own tests move with it). End state: `native/` and the pyo3 dependency are
   gone and `pyproject.toml` has nothing to build.
5. **One Tauri build per platform** in CI: Windows, Linux, macOS if you ship it (I did not
   check which you do). The installer, icons, the file associations and the updater, if any.
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

## Gate

Do not delete the Python app until a user's real setup has run the new one end to end:
sort, export, edit, build a patch, merge lands, and open the old queue and settings.
