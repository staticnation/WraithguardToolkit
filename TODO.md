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

## The Editor: the plan

In the order to do them. OpenMW-CS (`openmw-master/apps/opencs`) is a reference for what
a modder expects, never for code: it is GPL-3.

### 1. Foundations

- The contract's answers pinned for the links still without one (`"ans": null` in
  `viewer-shell/ui/tests/editor/contract.json`): editScript, editResult, editUses,
  editTopics, editLuaChart, editNewResponse and the dialogue views - with an example each
  that `tests/test_editor_contract.py` runs.
- One reader of the load order: the Editor session's reads moved to the Rust backend
  over time, so the engine and the session cannot disagree (the Rust migration).

### 6. The world

- Terrain editing: raise, lower, smooth and flatten with brushes; land textures painted;
  vertex colours painted (OpenMW-CS's three terrain modes). Merged Lands' land paths are
  the start.
- A region map: cells coloured by region, regions painted onto cells, exteriors made by
  dragging - from the cell map.
- Cell borders and names drawn in the render window beside the cell arrows.
- Path grid tools as OpenMW-CS has them: a connection dragged out from a point, points
  joined by a box.
- Remappable Editor shortcuts (a key binding page), with the controller bindings below.

## Controllers

`49_wg_gamepad.js` (fly / cursor modes, A switches), behind Settings' "Game controller"
switch (the viewer profile's `gamepad`); run from Steam (SteamGameId), cursor mode stays
off - the trackpad is the cursor. Still to do once tested on the Deck and with Robin: an
action layer and a binding table so buttons can be remapped (Unreal's
FViewportClientNavigationHelper split: inputs write impulses, the camera consumes them
once a tick); D-pad focus navigation for the menus; elsewhere move the real pointer where
the system allows (Tauri set_cursor_position: Windows, X11).

## OpenMW Lua tools

The Lua tools (see `CHANGELOG.md`) run in Rust (`viewer-shell/luacore`, shared by
`native/` and the viewer's engine; the Teal checker in `native/src/lua/check.rs`), with
the Python kept as the fallback and held to it by tests. Still to do:

- Each new OpenMW release: add its engine handlers and built-in events table
  (`lua/api.py`, from the release's `engine_handlers.html` and `events.html`; an install
  ships no .rst pages to read them from). 0.49, 0.50 and 0.51 are there.
- The Teal checks in the viewer too (htl in viewcore - a vendored Lua 5.4 in the viewer
  binary, so worth measuring first).
