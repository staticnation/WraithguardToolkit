# The Tools into Rust (plan)

Step 4.4 of the road in `TODO.md`: Merge Lands, Merge Settings, Lint, Save Check, tes3cmd,
Remove Master, Backups, the Lua view, the journal and plugin views, the help. Same method as
`PORT.md` (4.1), the load-order slices (4.2) and `CONFLICTS-PORT.md` (4.3): the logic
moves first, pinned by a case file or a golden output both sides run; Python calls it through
`native/`; then each tool is a Tauri command and a page, and its Tk window retires. Each
window is redesigned as it moves, not copied.

Draft from `TODO.md` and file sizes only; I have not read these tools' bodies. Re-read each
tool's code as its slice starts and correct the slice.

## What is already Rust

- Merge Lands: `native/src/land.rs` (772 lines, "most of it"), `viewcore/src/{land,mland}.rs`,
  `merge_to_master`. Python still has `wraithguard/land/` (about 7k lines: `service`,
  `seams`, `pipeline`, `emit`, `meta`, `native`, `landmass`, `slope`, `diff`, `merge`,
  `textures`, `heights`, `cleaning`, `curvature` ...).
- Lint: `native/src/lint.rs` (322 lines).
- The Lua view: `viewer-shell/luacore` (lexer, parser, analysis, scan, findings, mermaid,
  teal, ldt, omwscripts, report); `gui/luaview.py` (798 lines) is the window over it.
- Backups, BSA reading: `native/src/bsa.rs`; `scan_backups` is Python (`wraithguard_toolkit.py`).

## Slices (easiest first, so each ships alone)

1. **The Lua view page.** `luacore` has the analysis; this is the page and its Tauri
   commands, no new logic. Retire `gui/luaview.py` and `wraithguard/lua/*` once the report
   matches (`wraithguard/lua/report.py` against `luacore::report`, which is the case file).
2. **Lint.** `native/src/lint.rs` becomes a command; the Python caller goes. Pin: the
   findings list on a few real plugins.
3. **Backups.** `scan_backups`, the `.bak-<timestamp>` rules (name, listing, restore, the
   "unless backups are disabled" setting the export and patch writers read). Small and pure:
   a case file over a temp folder, as 2g did for the writes.
4. **Remove Master** (`gui/removemaster.py`, 392 lines). The plugin rewrite is editcore's
   plugin writer; port the rules (what refers to the master, what is left dangling) and pin them
   against output plugins byte for byte.
5. **Save Check.** Today it says whether a save's masters are missing; port that, then add
   what `openmw-save-stats` shows (MIT, a single stdlib Python file of about 1.4k lines; the
   copy Static sent is the reference): the space each mod takes in an `.omwsave`, split by
   part, its Lua data (keys, timers, the objects that carry it) and the hints to shrink it,
   and what grew between two saves. OpenMW save formats 37 and 40, 34 mostly. A crate of
   its own (`savecore`) or a module of `luacore`, which already reads OpenMW's Lua; decide
   when the slice starts.
6. **The journal and plugin views** (`journalview.py` 872, `pluginview.py` 1399). The journal
   is already editcore's (queue slice 3); the plugin view reads headers, masters and record
   counts, which the record cache has. These are mostly pages. Decide: whether the plugin
   view's per-plugin headers come from the cache or a fresh read (the TODO's "load-order
   tooling outside the editor" item says a large win from the cache).
7. **The help** (`viz/docs.py`, 549 lines). Becomes pages in the viewer; check what the
   viewer already has before moving text.
8. **tes3cmd** (`gui/t3.py`, 813 lines). Decide first: tes3cmd is an external Perl tool, so
   either the window stays a thin launcher over it (nothing to port, but then Perl is a
   dependency the one-program goal does not remove), or its clean and cleanup rules are
   rewritten over the tes3 crate, pinned against tes3cmd's own output. I would pick the
   rewrite only if you want no Perl; say which.
9. **Merge Settings** (`.mergedlands.toml`). Read and write the settings file, which
   Merge Lands and Lint read; ship it with 10 so both agree on one reader.
10. **Merge Lands**, last because it is the largest. Slice by the Python modules, each
    pinned against its own Python output on a fixed set of LAND records: heights and slope
    first (pure arithmetic), then seams, textures, cleaning, the landmass and the diff, then
    `emit` (the plugin written; byte for byte, as build was) and `service`/`pipeline` (the
    orchestration, which becomes a cancellable job). `native/land.rs` already covers much of
    this, so count what is left before slicing further.
11. **Python calls them through native**, then **the flip** per tool: a Tauri command, a
    Tools page, the Tk window and its mixin gone from `wraithguard_toolkit_gui.py`.

## Not ported / left

- The 4.4 tools' Tk layout (`widgets.py`, `theme.py`, `filedlg.py`): the page replaces
  them; see `DROP-TK.md`.
