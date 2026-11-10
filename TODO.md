# To do

Open work only. Done items go to `CHANGELOG.md`; what a finished item taught us goes
beside its code, or in `viewer-shell/gardenfell/VENDOR.md` for the vendored crates.
Code and decisions only: what is waiting to be tried on Static's setup is not kept here
(2026-11-09). Cleaned up 2026-11-02: the port to one program is done (5.0.0, in `CHANGELOG.md`), so its
history left this file; what was left of it is under "After 5.0" (2026-11-03, from the retired
`viewer-shell/DROP-TK.md`). Design notes that still guide open work: `viewer-shell/SOUND.md`,
`viewer-shell/GLAM-PLAN.md`, `viewer-shell/TES3CMD-REWRITE.md`, `viewer-shell/GARDENFELL.md`.

## Next: edit a plugin in place; MO2 and Morrowind.ini installs (Static, 2026-11-09)

Two features, in this order (Static: the Editor first).

### 1. The Editor edits an existing plugin: in (2026-11-10), to be tried in the game

In (`CHANGELOG.md` 5.0.4): Build patch's "Edit a plugin" (the Editor opens with it last) and
"Edit it in place" (`inplace::build_in_place`): the pool written into a plugin of the load
order, what it does not decide copied as its bytes, a deleted record only it had taken out,
a landscape's textures renumbered, a backup kept. Tried on the test plugins and on copies of
the game's masters, not in the game or on screen. Left:

- The journal does not name the plugin being edited: a pool made with one chosen is written
  into another if the choice is changed before the build. Name it in the journal, and ask.
- The record lists do not mark which records are the edited plugin's own.
- A record the plugin has twice: the last is the one replaced.
- The engine's world is not read again after the write, and it draws the load order as the
  game has it (the edited plugin where it stands, not last): Preview opens the result in a
  new Cell Preview, as for a patch.

### 2. One setup type for every page (reading only)

The viewer opens a `Data Files` folder, an MO2 profile or an `openmw.cfg`
(`viewcore::layout`, Robin's `gardenfell-install`: `ModOrganizer.ini`, the profile,
`modlist.txt`, `loadorder.txt`; the right `Morrowind.ini`, `[Game Files]`, the unticked
plugins by file date; the code page). `grassforge-main(2).zip` is that same code (its
`layout.rs` is ours byte for byte) and writes nothing. The Load order, Conflicts, Tools and
Editor pages take an `openmw.cfg` alone (`cf_setup`).

- A setup the pages read in place of the cfg: the plugins in order, the data folders by
  priority, the archives, what kind it is; made from a `Layout` or from the cfg. In
  `ordercore`, beside the cfg reader (the MO2 and Morrowind.ini load orders move there; the
  vendored crate stays Robin's, unmodified).
- Every page on an MO2 or Morrowind.ini install, read-only: Conflicts, Tools (lint, masters,
  the save check), the Editor, the command line (`--mo2 BASE [--profile NAME]`,
  `--data-files DIR` beside `--cfg`).
- What is OpenMW's alone hides or says so there: groundcover, Lua, `.omwaddon` and
  `.omwscripts`, the customizations TOML.

### 3. Sorting them: the mlox engine without MOMW

The engine takes plugins and rule files and gives an order; only what is around it is
OpenMW's (the curated list, `plugin-order.yml`, the customizations TOML, the cfg export).

- A mode where the whole load order is the user's to sort (plain mlox), the MOMW parts
  optional and off for these installs.
- Writers, in `ordercore` beside the cfg's, each with a dry run and a backup first:
  - MO2: the profile's `loadorder.txt` and `plugins.txt`.
  - Morrowind.ini: the `[Game Files]` list, and the plugins' modified dates (the game loads
    by date, masters first: what mlox and Wrye Mash set). Last, as it changes the user's
    plugin files: the dates before are kept, to put back.
- Hard parts: MO2's winner of a file is the mod's priority, not a `data=` line; a date set
  on a plugin MO2 manages lands in its mod folder; two plugins with one date.

## Tools page

- Merge Lands speed: Static's run of `big_order` on 2026-11-02 (before the merge step's
  work that day): read 0.37 s, merge 2.91 s, seams 0.68 s, write 0.25 s, the same plugin.
  The merge step since: no allocation per contested vertex, decoding across the cores, cells
  taken one at a time (2.15 s to 1.05 s here). Static's run after that (2026-11-02): merge
  2.23 s, seams 0.9 s, 4.47 s in all, the same SHA. Then: the decoded cells moved rather than
  cloned (peak memory 1.50 to 1.22 GB), a packed layer compared and read in place, the merge
  loop settling untouched vertices first - only 1% fewer instructions in the merge (callgrind):
  what is left is blending the 28 million contested vertices, the work itself. Next, if it
  still leads: the finish step (seams).
  Then the 621-plugin order on the Tools page. Benchmark, from `viewer-shell\landcore` (not
  a workspace member), on one line; prints the steps' times, VmHWM (Linux only) and the
  output's SHA-256 (`4d2e6cee...` on 2026-11-02):

      cargo test --release --test merge_cases -- --ignored --nocapture big_order

- tes3cmd in Rust (`viewer-shell/TES3CMD-REWRITE.md`; decided 2026-11-09, not started): its own
  repository (`staticnation/tes3cmd-rs`), a library and a `tes3cmd` command line over greatness7's
  `tes3`, with tes3cmd's own commands and flags so the MOMW Tools Pack and existing batch files
  can switch unchanged, and on Linux and macOS without Perl. tes3cmd is MIT (its header: "Copyright
  2016 by John Moonsugar ... under the MIT License"), so the Perl (`subset sort\tes3cmd`,
  0.40-PRE-RELEASE-2, 9,493 lines) is ported directly, its notice kept. Order: `clean` and what
  it rests on (records, masters and their cache, record comparison, the header) first; then
  `header`, `esp`/`esm`, `dump`, `delete`, `modify`, `diff`, `common`, `undelete`, `recover`;
  then `multipatch`, `fixit`, `overdial`, `lint`, `active`, `resetdates`. Not ported: `run` and
  `shell` (they run Perl), `codec`/`testcodec`. Pinned by tes3cmd's own output on fixture
  plugins, recorded once. Wraithguard takes it as a git dependency (as merge_to_master) and the
  Tools page drops the staging and the external program as each command lands.

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
- The viewer wearing the old settings' `theme` (`toolscore::settings` reads it) when its
  profile names none.
- Lint on the scan cache.
- Sound (`viewer-shell/SOUND.md`): Battle music (no fight to start it yet); `sound_peaks`
  cached by file (the page caches by path for the session).
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
- Phase 2c: the viewer's own scene keys (`i:Name` / `x,y` in `17_cell.js`, `18_cellpreview.js`,
  `32_cellhistory.js`, `50_wg_editor_ref.js`) - fold onto `CellKey.doorKey` only if one shows
  a case bug (`17_cell.js` lowercases the name, the others do not).
- The MO2 and Morrowind.ini load orders into ordercore: now step 2 of "Next" (top of this file).
- Why `editor_records` came back unreadable on Static's setup (Messages has the first 120
  characters of the answer).

### Editor stalls (Messages "Editor link ... took N s"): done, 2026-11-07

Done and measured (`CHANGELOG.md`, 5.0.4): the Editor's lock is not held across a plugin read, a
scan or a Verify; reads run on threads before it; cell names, ids, the use index, dialogue
and records are kept between launches. Verify went from over 6 minutes (357.8 s, the PC
pinned) to 2.7 s warm; the first open of a type is about 4 s cold and the next almost instant;
placing reads a cell's spelling from the file framing (0.10 s against 5.03 s of parsing the
cells of `TR_Mainland.esm`, same key); a topic is read from its own records (48 ms for 1,446
records, against every plugin's dialogue). What is left is small:

- Verify's game settings (`GMST`, 0.7 s of its 2.7 s) are read each launch; they could be kept
  on disk with the rest.
- Not done, on purpose: `read_tag` builds its values through JSON text (a record's `f32`
  fields would come out widened to `f64` from `serde_json::to_value`); `editRevision` is not
  answered without the lock (a stale revision would hide the refresh the poll is for).
- The first launch with no cache reads every plugin file from disk (about 45 s on Static's
  order, the disk, not the parsing): nothing to do but not to need it twice.

### Merged Lands and the caches the port may have lost (audit, 2026-11-07)

Found auditing the Python's caches against the port. The Editor's record cache and the key
index were the ones lost, and are back (`CHANGELOG.md`, 5.0.4).

- Merged Lands: `landcore::service::records_of` read each plugin twice, after a scan of every
  byte for `LAND`/`LTEX`; it now reads each once (`read::read_terrain_records`, tested to give
  the same records as the scan-then-read, and by the Python-parity merge cases). That is a
  saving of roughly a second over a load order of this size by a synthetic run, not measured on
  the real one. If the read is still a large share of a Merge Lands (`probe_land_read`,
  `tests/probe_land_read.rs`, measures it), keep each plugin's `LAND`/`LTEX` records on disk as the Python kept `<stem>.land.json` and
  asked the scan cache whether a plugin has terrain (`landcore` does not depend on `editcore`:
  a shared cache crate, or move the cache).
- Checked and kept: the NIF measurements (`nif_batch`, scan cache), the overlay's file
  index (`wraithguard_overlay_index.bin`), the conflict scan's keys, cells and terrain flags
  (`wraithguard_scan_cache.sqlite3`), the dialogue graph's lookups. Not needed (2026-11-09):
  the Lua archive scripts the Python unpacked into `wraithguard_lua_archives` (the engine
  reads them out of the archives where they are, `viewcore::luascan::VfsScripts` over the
  overlay), and the Conflicts window's `_conf_singles` and `_conflict_adjacency_cache` (a
  mode's rescan takes the cached keys, and the page counts a plugin's peers from the scan's
  list it already holds).
- `conflicts_verdicts` and `conflicts_plugin_group` read the scan's plugins again on each
  call (`cf_read_many`); Judge all calls the second once per plugin, so a plugin's records
  are read again for every plugin it shares one with. Both are timed in Messages
  ("Conflicts: judged ..."), 2026-11-09: read the times on the big order, and if Judge all
  is slow, keep each plugin's records of the scan's keys between calls, dropped with the scan.

### The record cache (`editcore::cache`)

- If the cache grows too big (the `uses` table is most of it): keep only the strings of
  fields that can name a record.
- Search all over a big order (2026-11-09): the hover on its count has the time. The
  matches' text is held until the answer is made (a common word over 621 plugins is tens of
  megabytes): if that shows, count past the first 2,000 fields without keeping them.

## Opening a big install (the viewer engine)

Measured on 621 plugins: read 5 s, parse 8.7 s, merge loop 40-47 s (exteriors 26-31 s,
interiors 9-10 s, tables 3-4 s), the file index 0.7-2.1 s from its saved copy.

- Exteriors, the biggest share of the merge loop (history about a third, the rest
  per-reference bookkeeping).
- A cache of the merged world only if that is not enough (a mappable flat file, invalidated
  on the whole load order).

## Release and CI

- Versions (sorted 2026-11-09 against GitHub): tags v5.0.0-v5.0.3 are pushed, and v5.0.3 is
  the only 5.x release published; `CHANGELOG.md` now has 5.0.1, 5.0.2, 5.0.3 and 5.0.4 (the
  work since v5.0.3). The version is 5.0.4 in
  `viewer-shell/Cargo.toml`, `viewer-shell/cli/Cargo.toml`, `viewer-shell/tauri.conf.json` and
  `viewer-shell/Cargo.lock` (2026-11-09; the v5.0.2 and v5.0.3 programs called themselves 5.0.1).
- The MOMW Tools Pack (Static's fork, `C:\tools\build\momw-tools-pack`; `get_wraithguard`
  in `build.sh`): rewritten for 5.0.1 with its sha256s, the AppImage in the Linux pack and the
  macOS binaries as `-macos-arm64`/`-macos-amd64` (1.58, not yet sent upstream). Move it to
  the release that is current when the merge request goes in (5.0.3 now: its four archives'
  and the AppImage's sha256s from its release page), then open the merge request.

- The dependency audit (`audit.yml`) is clean as of 2026-11-09: SQLite through `rusqlite` 0.41,
  `paste` gone with merge_to_master's `lending-iterator`, `glib` and `proc-macro-error` patched
  (below), `tokio` 1.54.1 and `yoke-derive` 0.8.4 taken earlier. It runs weekly: a new advisory
  fails it on a Monday with nothing changed here.
- gtk-0.18-patches (`viewer-shell/Cargo.toml` `[patch.crates-io]`, 2026-11-09). Each Tauri
  release: is `webkit2gtk` still in `viewer-shell/Cargo.lock`? When it is gone (a wry on
  WebKitGTK 6), drop the two patch lines and archive the repository.
- Actions are pinned to a commit of their current major; moving to the Node 24 majors
  (checkout 7, upload-artifact 7, cache 6) is a separate step: read each one's breaking
  changes, then re-pin.
- Tauri pins gtk 0.18 (glib patched, above), string_cache 0.9 and jsonptr 0.7 (Aikido: waiting
  on upstream); recheck each Tauri release.

- From the audit of 2026-11-09, not done:
  - `wg_post` and the `url` branch of the mesh viewer's save (`wg_save_edited`,
    `41_wg_meshview.js`) posted to the Python app's loopback server. No host serves them
    since 5.0, but the page tests' stand-in host answers the Editor's links over them
    (`ui/tests/boot.js`): move those tests onto `editor_link`, then remove both and the
    page's `links`. (`wg_open_record` had no caller left and is gone.)
  - `wg-view-serve` dispatches every command now but the three that start a program or end
    this one. The boot test still opens only the cell viewer, the mesh viewer and Setup: a
    boot of the Load order, Conflicts and Tools pages against the fixture setup would use
    the new arms.
  - The workspace (2026-11-09): copy `subset sort\workflows\` (`ci.yml`, `build-app.yml`,
    `audit.yml`, `osv-scanner.yml`, `deny.yml`) into `.github\workflows\` in the same commit as the
    workspace, or CI looks for the command line and the test engine where they no longer
    are; `deny.yml` there too (cargo-deny, `viewer-shell/deny.toml`). Then delete the old per-crate `target` folders (`viewer-shell\*\target`): only
    `viewer-shell\target` is used. First run on GitHub to watch: `build-app.yml` (the
    command line is `viewer-shell/target/release/wraithguard` now) and the AppImage.
  - `src/cellviewer/commands.rs` is split by page (`commands/`: conflicts, editor, help,
    land, loadorder, sound, tools) and is 4,200 lines: what is left is the viewer's own
    (the install, cells, meshes, textures, the distant view, Lua, settings). `loadorder_drop_probe`
    went with the Conflicts file and `editor_preview_patch` with Help, by where they stood:
    move each to its page's file.

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

- OpenMW Lua (`viewcore::luarun`, `74_wg_lua.js`): the camera, input bindings, console and
  HUD built-ins are left out (they need the game); `openmw.animation` answers but nothing
  is animated; `ui.create`'s keys go to a node once it is clicked.
- Physics drop (`76_wg_physics.js`, `physcore`): a hollow body is split coarsely (a 32-voxel
  grid, 16 parts at most), so a thin-walled vase may still fill in: a finer split if that
  shows.
- MWScript the preview still does not run: animations are named but not drawn (PlayGroup,
  LoopGroup: the mesh stands still), a spell's effects (Cast is counted for GetSpellEffects
  for ten seconds; GetEffect is not answered). Combat has no weapons, armor or fatigue.
  Actors move each time the scene is built again (about three times a second), so a walk
  steps rather than glides: gliding needs the renderer to move one drawn object between
  builds.

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
- Tell Greatness7: his crate refuses an NPC_ with a FILL subrecord that Robin's reads; one
  corpus mesh panics inside `bytemuck` (caught, falls back).
- `gardenfell-g7`'s differential tests on a full install (`GARDENFELL_G7_PLUGINS`,
  `GARDENFELL_G7_NIFS`): ARMO, CLOT, BOOK, ALCH, APPA, INGR, CONT, LOCK, PROB, REPA have no
  fixture.
- The text delta after Robin's string audit, and the rebase onto his release after that.

## Meshes and distant view

- "Optimise draw order": compare with G7's Blender exporter's output on the same mesh.
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

