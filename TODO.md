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
(highlighted source, syntax tree, findings, control-flow flowchart). Still to do:
- Teal: parsing and highlighting are done (Python). Type *checking* is the Teal
  compiler's job: run it through htl/mlua in Rust (`subset sort\htl-0.12.0`; checking
  and linting without luarocks), and Cyan (`cyan-0.4.1`, Teal's build tool) the same
  way for mods that ship a `tlconfig.lua`. Needs a Rust build to test.
- The Lua side in Rust (native or viewcore) as the Python prototype settles - LUAL via
  the tes3 crate's `ScriptConfigList`, which already reads LUAD too.
- Bundle mermaid.js (the flowchart loads it from a CDN today); call graphs per script
  (which local function calls which, as ast_mermaid's `calls`).
- Scripts in BSAs; the API version from the setup's OpenMW (only 0.51 / API 129 now).

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
