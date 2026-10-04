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
- **References**: move/rotate/scale in the render window (the CS's drag, Z/X axis locks,
  F to drop), the reference dialog (position, rotation, scale, ownership, lock, count).
  The writer is done and checked against merge_to_master (`wraithguard/patch/refedit.py`:
  a CELL record with the winning cell's own fields and only the changed references, keyed
  by the creating file's position in the patch's masters). Still to do: a `RefEdit` kind
  in the patch pool and `build_record_patch` (combined with field changes to the same
  cell), the render window's gizmo and the reference dialog, and new references
  (`next_new_index`, carried-forward builds included) and OpenMW's LUAL instance entries,
  which name references the same way and must be remapped with them.
- New records (Insert, or "new from this one" with a new id), deleting, renaming (with
  the CS's "used in N places" warning); placing a record from the Object Window into the
  render window (drag and drop).
- List fields in the dialog (inventory, spells, AI packages, leveled list entries,
  dialogue conditions) as tables, not read-only.
- Use Report (where a record is used: references, leveled lists, inventories, scripts),
  Search & Replace, the CSSE's Layers window and its Q context menu.
- Dialogue, and the Script Edit window (with the mwscript checks the toolkit has).
- An "active file" view: the patch being built, as the CS shows the plugin it edits.

## Controllers

`49_wg_gamepad.js` (fly / cursor modes, A switches). Still to do once tested on the Deck
and with Robin: put it behind a Settings switch; an action layer and a binding table so
buttons can be remapped (Unreal's FViewportClientNavigationHelper split: inputs write
impulses, the camera consumes them once a tick); D-pad focus navigation for the menus;
on Steam (SteamGameId) leave cursor mode off - the trackpad is the cursor; elsewhere move
the real pointer where the system allows (Tauri set_cursor_position: Windows, X11).

## OpenMW Lua tools

Started: `wraithguard/lua/` - `.omwscripts` reading, VFS resolution, a Lua 5.1/LuaJIT
lexer and parser (syntax tree), per-script analysis and load-order conflict and
performance checks against OpenMW 0.51 (API 129), as a CLI report
(`python -m wraithguard.lua openmw.cfg`), and the Conflicts window's "Lua scripts..."
(highlighted source, syntax tree, findings, control-flow flowchart, call graph, with
mermaid.js bundled). The API now comes from the setup's own OpenMW install (its LDT docs,
read in Rust, `native/src/lua`), written out as Teal declarations, and every script is
checked by the Teal compiler (htl 0.12.0 / tl 0.24.8, embedded). Still to do:
- A Settings field for the OpenMW install (found on its own now, or `--openmw` /
  `WG_OPENMW_RESOURCES`).
- Mods that ship a `tlconfig.lua` (Cyan projects): read its `include_dir`/`source_dir`
  so their own modules and declarations resolve.
- Type `openmw.interfaces` (a map of any now, since mods add interfaces): the built-in
  interfaces' docs are read, and each mod's `interfaceName`/`interface` table is too.
- Engine handlers and built-in events from the install as well (still the 0.51 list:
  the docs keep them in the .rst pages, not in LDT comments).
- The rest of the Lua side in Rust as the Python prototype settles - LUAL via the tes3
  crate's `ScriptConfigList`, which already reads LUAD too.
- Scripts in BSAs.

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

## Release notes for the next version

When the Unreleased section is released: rename it (PREFLIGHT.md, "Release"), and
write the GitHub release text from it - the water (MGE XE's own waves, dynamic
ripples, rain ripples, Wonders of Water), weather, bloom, shadows, the native merge,
the Flatpak (Steam Deck) and plain Linux build replacing the AppImage, controller
support, and that downloads are no longer minisign-signed (check the SHA-256 on the
Release page instead).
