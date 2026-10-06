# To do

Open work, in no particular order. Done items move to `CHANGELOG.md`.

## Forks: LUAL, Clippy

The build takes tes3 and merge_to_master from `staticnation/tes3` and
`staticnation/merge_to_master` (the `[patch]` sections in native/, viewer-shell/,
viewcore/ and check-commands' Cargo.toml). Both are pushed and tested: tes3 carries
Greatness7/tes3#7 (LUAL) and #9 (CI with Clippy, passing); merge_to_master carries the
LUAL merge (`types/lua.rs`) and the LUAD walker (`types/luad.rs`), with its
TB_BM dialogue test passing against a baseline recorded from upstream 5ea27f1
(`tests/make_baselines.rs`). Still to do:

- Offer merge_to_master's LUAL work upstream (a PR to Greatness7/merge_to_master, once
  tes3#7 is merged there), and ask greatness7 for the `ignore/` pairs behind the
  `MW`, `MW_TB`, `MW_BM` and `MW_TB_BM` dialogue tests.
- When both are upstream: delete the `[patch]` sections and point merge_to_master back
  at Greatness7.

## The Editor mode (a Construction Set beside the render window)

Started: the viewer's Editor mode (`viewer-shell/ui/src/50_wg_editor.js`) - the Object
Window (record tabs, filter, sortable columns, changed records marked), the Cell View
(cells and their references), a dialog per record whose changes go to Wraithguard's patch
pool (`wraithguard/patch/editor.py`, `gui/editorlink.py`), the pool journalled so a crash
loses nothing. Still to do, roughly in this order:
- **References**: done - the writer (`wraithguard/patch/refedit.py`, checked against
  merge_to_master), changed references in the pool and `build_record_patch`, the
  reference dialog (a door's destination included), moving objects in the render window,
  `moved_cell` when one crosses an exterior edge, and new references placed from the
  Object Window (the patch's own `(0, n)`, numbered on from an earlier build's); OpenMW's
  LUAL lists carried with their LUAI entries and Lua data renumbered (`lual_remap`).
- Records: done - "Make a copy as", "New" (a blank record of a type), Delete/Undelete
  (the DELETED flag) and "Rename to" (a copy, the live uses repointed to it, the count
  said); a copied script's `begin` line takes the new id; a topic copied whole.
- List fields: done (tables in the record dialog; a group per entry - a dialogue
  condition, an AI package - a column per field, each entry checked against one of its
  kind). Values nested deeper than that are JSON in their cell.
- Use Report, Search & Replace (fields and placed references), Layers and the Q menu:
  done, Search & Replace in scripts too (recompiled); the Layers and Q menu checked against
  CSSE's documentation (see the Script Edit entry for what is left).
- Script Edit window: done (source checked as typed, compiled listing, saved to the
  pool). Saving compiles (the Rust port of MWEdit's compiler) and queues the bytecode;
  extended functions, results checked, Search & Replace in scripts, a script's flowchart
  and the checks in Rust are done; CSSE's layer tint and Ctrl+click multi-selection with
  the Q menu's align/randomize too.
- Dialogue window: done (topics by kind, responses in engine order, a response edited in
  the record dialog and written inside its topic; a new response added at a place in a
  topic; a journal's stages by index under its quest's name).
- Active file view: done ("Patch only" in the Object Window, tabs counting, cells marked).

## Editor: asked for by modders (UESP Discord, 2026-10)

- Pathgrid editing and the navmesh: done (the Editor's Path grid mode; Tools' Navmesh
  overlay reading `navmesh.db`). Still to do: the navmesh's off-mesh links (doors,
  jumps) drawn, a tile's age against the plugins (navmeshtool's `input` holds the
  geometry it was built from), and generating points from the navmesh's polygons.
- Cell duplication ("Make a copy as" for a whole cell and its references).
- Search & Replace: "Replace all in the current cell" beside "every live use" and "the
  selected".
- CSSE's filter/search across the Object Window (by any field, not only id/name/model).
- Less dirtying: the patch already carries only what changed, and dialogue only the
  responses touched (with their position anchors); say so in the Build patch report, and
  flag a change that puts a field back to the winning value as no change at all.
- Dialogue sorting (done: the Dialogue window's Flow view shows the engine's order and
  what falls through; still to do: drag to reorder, written as `prev_id` changes).

## Controllers

`49_wg_gamepad.js` (fly / cursor modes, A switches), behind Settings' "Game controller"
switch (the viewer profile's `gamepad`); run from Steam (SteamGameId), cursor mode stays
off - the trackpad is the cursor. Still to do once tested on the Deck and with Robin: an
action layer and a binding table so buttons can be remapped (Unreal's
FViewportClientNavigationHelper split: inputs write impulses, the camera consumes them
once a tick); D-pad focus navigation for the menus; elsewhere move the real pointer where
the system allows (Tauri set_cursor_position: Windows, X11).

## OpenMW Lua tools

Started: `wraithguard/lua/` - `.omwscripts` reading, VFS resolution, a Lua 5.1/LuaJIT
lexer and parser (syntax tree), per-script analysis and load-order conflict and
performance checks against OpenMW 0.51 (API 129), as a CLI report
(`python -m wraithguard.lua openmw.cfg`), and the Conflicts window's "Lua scripts..."
(highlighted source, syntax tree, findings, control-flow flowchart, call graph, with
mermaid.js bundled). The API now comes from the setup's own OpenMW install (its LDT docs,
read in Rust, `native/src/lua`), written out as Teal declarations, and every script is
checked by the Teal compiler (htl 0.12.0 / tl 0.24.8, embedded). Still to do:
- Engine handlers and built-in events: a table per release now (`lua/api.py`: 0.49, 0.50,
  0.51, from each release's reference pages), picked by the install's version. Each new
  OpenMW release needs its table added (its `engine_handlers.html` and `events.html`); an
  install ships no .rst pages to read them from.
- The Lua side is in Rust (`viewer-shell/luacore`, shared by `native/` and the viewer's
  engine; the Teal checker in `native/src/lua/check.rs`), the Python kept as the fallback
  and held to it by tests. The Editor's Lua panel scans without Python (`lua_scan`). Still
  to do: the Teal checks in the viewer too (htl in viewcore - a vendored Lua 5.4 in the
  viewer binary, so worth measuring first), and a script's flowchart from the panel.

The load order's Lua scripts, read from each mod's `.omwscripts` lists (plain text,
`CONTEXT: path` lines): a script browser with syntax highlighting; a parsed tree (AST)
for each script; conflict detection (two mods registering the same script path, the
same interface name, the same event or engine handler, or overriding each other's
files in the VFS); and performance diagnostics (per-frame handlers such as `onUpdate`
and `onFrame`, loops over `nearby.actors` / `world.activeActors` / `types.*.records`,
queries run every frame). The OpenMW Lua API changes between releases, so API checks
should follow the version the setup's OpenMW reports rather than one fixed list.
Reference: https://openmw.readthedocs.io/en/openmw-0.51.0/reference/lua-scripting/api.html
(one page per OpenMW release; pick the one matching the setup).
Teal (a typed dialect of Lua that compiles to plain Lua) is worth supporting beside
plain Lua, for mods written in it: https://teal-language.org/book/latest/index.html
The Teal compiler's source (tl 0.24.8, MIT) is beside the repository in
`subset sort\tl-0.24.8`, and its build tool Cyan (0.4.1) in `subset sort\cyan-0.4.1`.
htl 0.12.0 (`subset sort\htl-0.12.0`, https://github.com/ynishi/htl, MIT or Apache-2.0) runs Teal from Rust through
mlua - type checking, linting and formatting without luarocks - which would let the
Rust side (native or viewcore) check scripts without a Lua install.
