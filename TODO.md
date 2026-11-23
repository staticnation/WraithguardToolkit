# To do

Open work only. Done items go to `CHANGELOG.md`; what a finished item taught us goes
beside its code, or in `viewer-shell/gardenfell/VENDOR.md` for the vendored crates.
Code and decisions only: what is waiting to be tried on Static's setup is not kept here
(2026-11-09). Cleaned up 2026-11-17: finished work, its measurements and status checks left
for `CHANGELOG.md`; a decision stays as one line where open work depends on it. Design notes that still guide open work:
`viewer-shell/GLAM-PLAN.md`, `viewer-shell/TES3CMD-REWRITE.md`, `viewer-shell/GARDENFELL.md`.

## Security audit (2026-11-20, and the second of 2026-11-23)

Done in 5.0.6: every finding of both audits (see `CHANGELOG.md`): F1-F9, the second audit's 1-6 and its F5
and F7, the innerHTML lint, LuaJIT tested in CI, the Lua in a worker process the program ends, and build
provenance. `cargo audit`, `cargo deny check` and OSV-Scanner pass in CI as of 2026-11-23. Left, by choice
or because it waits on something outside the code:

- **F4, kept by choice:** the rule files follow `main` of a third-party repository, not a pinned commit
  (Static, 2026-11-23: new rules arrive without a release). What that trusts: whoever can push to that
  branch decides what the sort and the warnings say. The rules are data (read by the YAML and rule
  readers, never run), so the reach is wrong advice, not code.
- **Attestation is in `workflows/`, not yet in `.github/workflows`** (copy it there; `.github` is not
  edited from here). It attests public builds only (`!github.event.repository.private`); the builds are not
  signed. The WebView2 pin does nothing until `WEBVIEW2_FIXED_SHA256` is set. The checksums and `--locked`
  are untested until a tag.
- **F5's edges:** a plugin written over in place is accepted when it exists and is named as a plugin (the
  writers read it as one first); the Editor's preview setup is written beside the openmw.cfg the page names,
  under the program's own name. Reads are not limited (the page names what to show).
- **The innerHTML lint keeps its 205 known places** (`ui/tests/html_sinks.baseline.json`); they were read
  as escaped or the page's own text, not proven so. A UI test that loads records named with markup is not
  written.
- **The Lua worker** has a time limit, not a memory one: a script's allocations in C are bounded by the
  prelude's memory check between instructions and by the machine. With LuaJIT's compiler on (2026-11-23),
  a compiled loop is past the hook's time and memory checks: it runs until the worker's 15 s deadline,
  allocating as it likes meanwhile. A memory cap on the worker process itself (a Job object on Windows,
  `setrlimit` elsewhere) would close that.
- **MWSE in the Preview** runs main.lua's events, timers, config and the core tes3 calls; the rest of MWSE
  (tes3ui menus, mobile actors' stats, the MCM, files, `mge`) answers "not run in the preview". Events
  the Preview does not make (`keyDown`, `damage`, `menuEnter`...) are never fired.

## Lua API coverage (Static 2026-11-23: "every call defined")

- **OpenMW: done for the surface.** Every documented call is defined (`api_fill`, from
  `lua_api/openmw.json`). Next: real behavior for what is only answered by type. In the packages a global
  script requires, 39 module calls are (`WG_API_REPORT=1 cargo test -p viewcore every_documented`
  lists them: `Debug` 10, `menu` 9, `world` 6, `util` 5, ...); the type tables (`types.Actor`, `NPC`,
  `Item`...: 220 documented calls) and the player-only packages (`self`, `nearby`, `camera`, `input`,
  `ui`, `ambient`) are not counted yet - the test runs a global script only.
- **MWSE: done for the surface** (`lua_api/mwse.json`, from a local copy of MWSE-master, 2026-11-23:
  re-run `tools/mwse_api_surface.py` on a newer one). Next: real behavior for more of `tes3` on the scene
  (`tes3.getObject` for every record, not only what is drawn; `tes3.createReference`, `tes3.getCell`,
  inventories, mobiles' stats from the records) and the events the Preview could make (`keyDown`,
  `uiActivated` for its own windows, `combatStarted` from the AI). Most of MWSE reads and writes
  Morrowind.exe's memory and can only ever be answered by type here. MWSE's source is GPL: only
  names, types and the game's constants are taken, never code.

## Tools page

- Merge Lands speed. Benchmark, from `viewer-shell` (prints each step's time, the peak memory and
  the output's SHA-256, `d3c09862...` as of 2026-11-17):

      cargo test --release -p landcore --test merge_cases -- --ignored --nocapture big_order

  Static's real order (619 plugins, 2026-11-10) spent 4.4 s between "declaring" and "wrote" (13.5
  MB); Verbose now splits the encoding from the disk write. A Verbose run of the real order says
  whether that time is the disk (or a scanner on the new file) or the encoding. Not the plugin
  lookups: those go through one index of the data folders (`PluginIndex`). The synthetic order
  (2026-11-18): 2.2 s whole, encoded in 0.09 s, written in 0.03 s.
- tes3cmd (built in since 2026-11-17, `viewer-shell/TES3CMD-REWRITE.md`). Left on purpose:
  `modify --run` and `--program-file` run Perl, so they stay the external tes3cmd's; the master
  cache is not ported (it holds every record of the master, so reading it back costs what reading
  the master does).

## Merge Lands

Open:

- The real order through this build (the terrain audit, `TERRAIN_AUDIT_2026-11-11.md`, is done on
  its 74 decoded inputs: `audit1.7z` `decoded/inputs`, numbered as `geometry/inputs.json`'s
  plugins).
- The existing-seams repair (`--merge-repair-seams [GAP]`, opt-in, gap 24): no ceiling above 24
  has been shown safe (600 on the audit's order made three borders worse and clamped two
  vertices). Three borders of 24 units or less stay open on the audit's order, each one corner
  vertex where the cells outside the merge that share it disagree with each other ((13,-30)/
  (14,-30), its northern neighbors at 1,448 and 1,424; (-116,12)/(-115,12); (-12,-15)/(-11,-15)):
  closing them means moving terrain outside the merge. The audit's note stands: no record count
  cut by suppressing small mismatches; known intentional discontinuities go in as exceptions.
  Record counts against Merged Lands are not a measure.
- Memory, if a bigger order needs it: the merge's own peak on `big_order` is 451 MB (2026-11-17),
  reached in seam repair; most of it is what is read (the reference and the loaded landmass, the
  decoded cells, each cell's normal sources until the normals step, 190 MB). A merged cell's
  heights are still `i32` (17 KB a cell). The process's figure (VmHWM) moves by up to 130 MB
  between runs of the same merge: compare the log's "added by the merge".

Decided, and different from OpenMWMergedLands on purpose (Static, 2026-11-16):

- The step repair runs after the slope limiter (a hybrid): the limiter shares a cliff out first,
  the fork's lowering takes what it refused. `--merge-no-clamp` stays for a repair given up.
- A cell the step repair brings in keeps its own phase; where it differs from the piece's, the
  lowered vertex is floored onto it by the encoder (said as a border a few units apart).
- The world map is made again where the ground under it moved; the fork's way is
  `--merge-keep-world-map` ("Keep world maps as merged").
- The fork's `merged_lands.toml` is read only when asked (`--merge-fork-config`, "Import from
  OpenMWMergedLands"), once, into Wraithguard's settings.
- Its `LTEX` order is ours but for its end pass over previous merges and deleted `LTEX` records:
  left as it is (2026-11-17).
- Only here, kept as options: `Curvature`, the masters model, the existing-seams repair, the
  Python-recorded cases.

Plain Morrowind order (decided 2026-11-11/12): the game sorts the ini's plugins by file date,
masters first (`ordercore::install` and the viewer's connect sort that way); `gardenfell-install`
shows the ini's order and changes the files' dates to match it (`ordercore::plain::dates_for`). A
master is told by its extension everywhere; the master check only notes a header that disagrees.

## The viewer

- Orbit navigation without an end (2026-11-16): serviceable, wants polish (nothing named yet). The
  renderer's mouse handling has no page test (it needs the shell to move the pointer and a GL
  context).
- Scripted actors glide (`18_cellpreview.js` `previewMoveRef`, 2026-11-19): while the Scripts run,
  an actor's batches draw on their own (`live`, not in the material groups), and a move writes
  its matrix into them, widens its cull run and moves its pickable, with no rebuild. **Needs a
  look in the app**: an NPC walking a town street should glide, and stay drawn as it crosses
  tiles. Not carried along between rebuilds: a carried torch's light and flame, and a static
  or activator a script moves (they still step, a few times a second).

## After 5.0

Decided (Static, 2026-10-26): the command line stays, in Rust; translations keep the `.po`
files as their source; the platforms are Windows, Linux, macOS, Android and iOS.

- Translations: English only until everything is in and working (Static, 2026-10-27). Then
  the page's string tables generated from the `.po` files in `locale/`, and the page's own
  strings extracted into the same `.pot` (`locale/README.md`).
- Android and iOS (Tauri 2's mobile targets). First an audit of what the page and the
  commands assume of a desktop: spawned processes (`editor_preview_patch`, tes3cmd, the
  OpenMW test run), free file paths and `openmw.cfg` locations (OpenMW's Android app keeps
  its files elsewhere), window management, hover-only controls, the gamepad and keyboard
  paths. Then the builds in CI and a device test reading a real install.
- An installer (the WebView2-bundled Windows package is in).
- Lint on the scan cache.
- First person: the encumbrance from what the body wears rather than a slider.
- glam (`viewer-shell/GLAM-PLAN.md`, its rules first): the NIF transform pipeline
  (`gardenfell-nif/src/nif.rs`), tests against the current code and real fixtures before
  any change.

## The Editor

OpenMW-CS (`openmw-master/apps/opencs`) is a reference for what a modder expects, never
for code: it is GPL-3.

### Foundations

- Phase 2b is decided (Static, 2026-11-09: as the Construction Set): a later plugin's plain
  record for a moved reference is the reference, at home (Gardenfell patch 0023). Not seen in
  the Construction Set or the game itself: `WG_Move_B.esp` moves a rock and `WG_Move_C.esp`
  edits its scale at home - load both in the CS and see where the rock stands; if it stands
  where B moved it, 0023 and the Editor's rule turn round together
  (`tests/fixtures/move_cases.json`, "moved, then a plain record in its home cell").
- **A mask for landscape texture painting and vertex color** (Static, 2026-11-18: "we can do that
  later"): an area the brushes (`72_wg_landbrush.js`) paint inside, or are kept out of.
- Phase 2c: the viewer's own scene keys (`i:Name` / `x,y` in `17_cell.js`, `18_cellpreview.js`,
  `32_cellhistory.js`, `50_wg_editor_ref.js`) - fold onto `CellKey.doorKey` only if one shows
  a case bug (`17_cell.js` lowercases the name, the others do not).

### Speed and caches

- Verify's game settings (0.7 s of its 2.7 s on Static's order, 2026-11-07): a launch now skips the
  plugins the key index says have none (2026-11-17, `editcore/tests/warm_settings.rs`). Time
  Verify again on the real order.
- Merge Lands reads each plugin once (`read::read_terrain_records`). If the read is still a large
  share of a merge (`probe_land_read`, `tests/probe_land_read.rs`, measures it): keep each
  plugin's `LAND`/`LTEX` records on disk as the Python kept `<stem>.land.json`, and ask the scan
  cache whether a plugin has terrain (`landcore` does not depend on `editcore`: a shared cache
  crate, or move the cache).
- Not done, on purpose: `read_tag` builds its values through JSON text (a record's `f32` fields
  would come out widened to `f64` from `serde_json::to_value`); `editRevision` is not answered
  without the lock (a stale revision would hide the refresh the poll is for).

## Opening a big install (the viewer engine)

Measured on 621 plugins: read 5 s, parse 8.7 s, merge loop 40-47 s (exteriors 26-31 s,
interiors 9-10 s, tables 3-4 s), the file index 0.7-2.1 s from its saved copy.

- Exteriors, the biggest share of the merge loop (history about a third, the rest
  per-reference bookkeeping).
- A cache of the merged world only if that is not enough (a mappable flat file, invalidated
  on the whole load order).

## Release and CI

- The MOMW Tools Pack (Static's fork, `C:\tools\build\momw-tools-pack`; `get_wraithguard` in
  `build.sh`): rewritten for 5.0.1 with its sha256s, the AppImage in the Linux pack and the macOS
  binaries as `-macos-arm64`/`-macos-amd64` (1.58, not yet sent upstream). Move it to the release
  that is current when the merge request goes in (its four archives' and the AppImage's sha256s
  from the release page; the AppImage is `Wraithguard_Toolkit-x86_64.AppImage` from 5.0.5), then
  open the merge request.
- AppImage catalog PR (`appimage.github.io#11424`): the asset is `Wraithguard_Toolkit-x86_64.AppImage`
  from 5.0.5 (the catalog wants no "linux" in the name; `build-app.yml` changed in
  `subset sort\workflows`, to copy into `.github/workflows`). Left: the PR's data file, if it names
  the asset. The window keeps the program's own themes (dark by default on a light system; decided
  2026-11-17), so the catalog's warning about it stands.
- gtk-0.18-patches (`viewer-shell/Cargo.toml` `[patch.crates-io]`). Each Tauri release: is
  `webkit2gtk` still in `viewer-shell/Cargo.lock`? When it is gone (a wry on WebKitGTK 6), drop the
  two patch lines and archive the repository. Tauri also pins string_cache 0.9 and jsonptr 0.7
  (Aikido: waiting on upstream): recheck each Tauri release. Last rechecked with Tauri 2.12.7
  (2026-11-18): `webkit2gtk`, string_cache 0.9 and jsonptr 0.7 are all still there.
- Actions re-pinned 2026-11-17 in `subset sort\workflows`, to copy into `.github/workflows`:
  checkout 7.0.1, upload-artifact 7.0.2, cache 6.1.0, setup-node 7.1.0 (all were Node 24 already;
  the new majors are their move to ES modules, and checkout 7's block on fork PRs is for
  `pull_request_target` and `workflow_run`, which no workflow here uses). Check the first CI run.
- The dependency audit (`audit.yml`) runs weekly: a new advisory fails it on a Monday with nothing
  changed here.
- Held by others' pins (2026-11-17): generic-array 0.14.7 (`crypto-common` 0.1.7 takes `=0.14.7`,
  through wry's sha2 0.10) and lua-src 551.0 (`mlua-sys` 0.13.0 takes `<551.1.0`). They move with
  wry and mlua.
- toml 0.8 is still in the build, through htl-core (0.19 too); it goes when htl does.
- chrono is unmaintained (RUSTSEC-2026-0543, informational; ignored in `viewer-shell/osv-scanner.toml`
  2026-11-17). Nothing builds it (the command line is on `time`), but `Cargo.lock` keeps it as an
  optional dependency of `serde_with` (through `tauri-utils`), and OSV reads the lock. The entry
  goes when `serde_with` drops it or Tauri drops `serde_with`.

## Forks: LUAL, Clippy

tes3 and merge_to_master come from `staticnation/tes3` and `staticnation/merge_to_master`
(the `[patch]` sections in the crates' Cargo.toml).

- Offer merge_to_master's LUAL work upstream (once tes3#7 is merged), and ask greatness7 for
  the `ignore/` pairs behind the `MW`, `MW_TB`, `MW_BM` and `MW_TB_BM` dialogue tests.
- When both are upstream: delete the `[patch]` sections.
- Greatness7: the NIF field-swap fix goes into his next tes3 update (drop it from our fork
  then); LUAL/LUAD waits for his next breaking release. Show what a LUAL/LUAD carries before
  it goes into a patch.

## Scripts in the scene (`73_wg_scripts.js`)

- OpenMW Lua, Static's 2026-11-19 log: the settings defaults and the enums are answered now.
  Not confirmed against the mods themselves (their folder was not readable from here): run the
  Scripts again and see which of fresh-loot, MRF, PerfectPlacement, steelfist, SBMR,
  BetterArmorTraining, UIToolkit tooltips and go-home still fail, and at which line.
- OpenMW Lua (`viewcore::luarun`, `74_wg_lua.js`): the camera, input bindings, console and
  HUD built-ins are left out (they need the game); `ui.create`'s keys go to a node once it is
  clicked. `openmw.animation` does not crossfade (a group snaps in from the pose before; a
  renderer change that needs a look in the viewer), and `addVfx` is not drawn.
- **MWSE-Lua** for Morrowind.ini and MO2 setups (Static, 2026-11-18): `mwse.*`, `tes3.*` and
  `event.register`, run against the same scene layer as MWScript and OpenMW Lua, with the
  animation groups and Which scripts run shared. Not started.
- AI (`77_wg_ai.js`): a live actor's change of animation swaps its own mesh alone
  (`previewSwapActor`); an actor that is not live (Scripts off when the scene was built) still
  waits for the scene to be drawn again, its AI changes gathered up every 1.5 s. Walks and
  idles are played within 4,096 units of the camera.
  Fleeing, greeting and keeping room are approximations of OpenMW's (no disposition, no
  greeting voice, no actor collision past the room kept).
- MWScript the preview still does not run: a spell's effects (Cast is counted for GetSpellEffects
  for ten seconds; GetEffect is not answered). Combat has no weapons, armor or fatigue.
  A torch an actor carries stays where it was until the scene is drawn again (The viewer, above).

## MWScript compiler

**Wait for G7:** his compiler goes into the tes3 crate; then drop our MWEdit port
(`editcore::mwscript`) after running both on `MwEditScriptTest.esp`. PT, TR and OAAB still use
the vanilla CS, so then:

- Warn in Script Edit when a script would not round-trip through TESCS; mark the scripts a
  patch recompiled; never recompile one it did not change.
- Tests: TESCS resolves an id against every script's locals first (`GetSoundPlaying "Drink"`
  vs `bittercup`'s `drink`); a comma in the wrong place breaks silently in the CS. A corpus
  where TESCS and MWEdit disagree (ask on the MMC).
- Checker warnings (morromod.htm): `set x to x + 1` without parentheses; a local another
  script declares but this one does not; `SetHealth`/`Mod*` on a remote variable or missing
  `mycompanion.`; `StopScript ItsOwnName`; MWEdit's misses (`RemoteLocalVar` in targeted
  scripts, region names with `PositionCell`, anything under `Choice` in a result).

## Gardenfell (`viewer-shell/GARDENFELL.md`, `viewer-shell/gardenfell/VENDOR.md`)

- Send Robin `subset sort\Gardenfell-offering-2026-10-19.zip` (0001-0014). His `fixture/`
  stays out of the repo; put it in `viewer-shell/gardenfell/fixture` locally to run his suite.
- Offer Greatness7 our tes3 fork's four fixes, each with its write-up in
  `subset sort\tes3-patches`. Our fork has them since 7912cc2 (710b0e2 with its formatting), and
  WG's `Cargo.lock` takes 710b0e2:
  - `bytes_io-length-bounds`: the `bytemuck` panic on a misread length;
  - `esp-lenient-unknown-subrecords`: an NPC_ with a FILL;
  - `nif-uv-sets-by-count`: the corpus meshes that failed, which had their `Has UV` word 0;
  - `reserve-count-bounds`: counts that aborted the process.
- Send Robin 0024 (UV sets by their count, `stream::walk`, `netimmerse::repair`) with the next
  offering.
- `gardenfell-g7`'s differential tests on a full install (`GARDENFELL_G7_PLUGINS`,
  `GARDENFELL_G7_NIFS`): ARMO, CLOT, BOOK, ALCH, APPA, INGR, CONT, LOCK, PROB, REPA have no
  fixture.
- The text delta after Robin's string audit, and the rebase onto his release after that.

## Meshes and distant view

- "Optimize draw order": compare with G7's Blender exporter's output on the same mesh.
- 40 corpus meshes are NIF 4.0.0.0 (most from Properly Smoothed Meshes). Find out what the game
  and OpenMW do with them, and read them if they load.
- Check meshes' guess repair (`dbs_meatstick.nif`'s two "Emitter" roots): see the mesh in
  OpenMW before and after, and whether the game loads the original.
- From MGE XE, not taken yet: VRAM streaming with eviction, a frame-budgeted upload pump,
  horizon culling of terrain, static texture atlases, building classification.

## Controllers

`49_wg_gamepad.js`, behind Settings' "Game controller"; its buttons are remapped on the
key binding page (`69_wg_keys.js`) since 2026-11-02. Left: D-pad focus navigation for the
menus; the real pointer moved where the system allows (Tauri
`set_cursor_position`).

## OpenMW Lua

- Each new OpenMW release: its engine handlers and built-in events (`luacore::api`, from the
  release's `engine_handlers.html` and `events.html`). 0.49-0.52 are there; 0.52's came from
  Static's 0.52.0 RC1 `resources` (commit 4ff261bf), so when 0.52 is released (or a later RC): its
  `API_REVISION` (166 for now, RC1's own, from its CMakeLists.txt; the Preview gives scripts
  the install's, by its `resources/version`) and its pages, for handlers and events its scripts do not
  show. Check `WG_OPENMW_RESOURCES=<resources> cargo test -p luacore an_install -- --nocapture`.

