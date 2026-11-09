# To do

Open work only. Done items go to `CHANGELOG.md`; what a finished item taught us goes
beside its code, or in `viewer-shell/gardenfell/VENDOR.md` for the vendored crates.
Cleaned up 2026-11-02: the port to one program is done (5.0.0, in `CHANGELOG.md`), so its
history left this file; what was left of it is under "After 5.0" (2026-11-03, from the retired
`viewer-shell/DROP-TK.md`). Design notes that still guide open work: `viewer-shell/SOUND.md`,
`viewer-shell/GLAM-PLAN.md`, `viewer-shell/TES3CMD-REWRITE.md`, `viewer-shell/GARDENFELL.md`.

## This week (Static, 2026-11-02): landscape editing and sound

Done this week (in `CHANGELOG.md`, 5.0.1): the Land view, Merge to master, sounds on every
field and in place, the world's sound and music, the first-person camera - each tried on
Static's setup. Left:

### Landscape brushes and overlays (`67_wg_land.js`, `72_wg_landbrush.js`)

- Try on the real setup: each tool and preset, a click vs a drag vs a hold, the overlays
  (relief map, rainbow, contours) over a real region; the ring's outline for a square,
  diamond or narrow brush turned to an angle.

### First person (`06_gl.js` `_walkStep`, `71_wg_walk.js`)

- Try on the real setup: the pace against the game's with your own save (Body, From a
  save...; `Mage_Begining.omwsave`, format 39, reads as Speed 32, Athletics 5, Acrobatics 0);
  swimming and levitating; the steps, swim strokes and landings with Sounds on; collision on a few
  meshes with and without a RootCollisionNode (and an NCO one walked through).
- The controller's sticks (untested); the encumbrance from what the body wears rather than
  a slider.

## Command line (`viewer-shell/cli`)

- Merge Lands from the command line (Sophie, 2026-11-09: "How do I use your merged lands
  cli?"). There is none: the 4.x Python had none either, and the 5.x `wraithguard` sorts,
  exports, scans conflicts, lints and draws the cell map only. `landcore::service` already
  runs the whole merge as one call with the Tools page's progress log: a flag (or
  subcommand) over it, its output path and the `.mergedlands.toml` settings as on the Tools
  page, a case in `tests/fixtures/cli_cases.json`, and README's command-line section.
- README's "Coming from 4.x?": say to remove the old pip-installed launcher
  (`pip uninstall wraithguard-toolkit`). Left on PATH, `wraithguard` in a terminal runs it,
  and it fails with "No module named 'wraithguard_toolkit'" (Static, 2026-11-09).

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

- tes3cmd rewritten in Rust (`viewer-shell/TES3CMD-REWRITE.md`).

## After 5.0

Decided (Static, 2026-10-26): the command line stays, in Rust; translations keep the `.po`
files as their source; the platforms are Windows, Linux, macOS, Android and iOS.

- Translations: English only until everything is in and working (Static, 2026-10-27). Then
  the page's string tables generated from the `.po` files in `locale/`, and the page's own
  strings extracted into the same `.pot` (`locale/README.md`).
- The AppImage (`build-app.yml`'s `linux-appimage`, replacing the Flatpak): check the first
  build on a desktop distro and on the Deck (Desktop Mode, and Game Mode as a non-Steam game),
  that its sound plays (the bundled GStreamer), and that settings land beside the .AppImage.
  Then remove `build-flatpak.yml` and `packaging/flatpak/`.
- A CSP: `tauri.conf.json` has `"csp": null`. The page's inline `on...=` handlers move to
  `addEventListener` first (recount them before scoping), then the policy is set, the
  `WG_TAINT` runs go once more, and Aikido's finding is marked.
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
- glam (`viewer-shell/GLAM-PLAN.md`, its rules first): the NIF transform pipeline
  (`gardenfell-nif/src/nif.rs`), tests against the current code and real fixtures before
  any change.

## The Editor

OpenMW-CS (`openmw-master/apps/opencs`) is a reference for what a modder expects, never
for code: it is GPL-3.

### Foundations

- Confirm phase 2a in the game: `wg_clash_test/WG_Clash_A.esp` (wg_clash as a weapon) then
  `WG_Clash_B.esp` (a misc item, and the cell `WG Clash Test` placing it). Expected: the
  placed one is the weapon; `player->additem wg_clash 1` gives the weapon.
- Phase 2b, Static decides: a reference moved by one plugin (MVRF), then given a plain
  record in its home cell by a later plugin. The engine keeps it in both cells; the Editor
  puts it back home. Options: (a) home - Gardenfell patch 0018 (`leave()` for every record)
  and a note for Robin; (b) the move stands and the plain record only edits it. Test in the
  game first (`WG_Move_B.esp` moves a rock, `WG_Move_C.esp` edits its scale at home). Pinned:
  `tests/fixtures/move_cases.json`, "moved, then a plain record in its home cell".
- Phase 2c: the viewer's own scene keys (`i:Name` / `x,y` in `17_cell.js`, `18_cellpreview.js`,
  `32_cellhistory.js`, `50_wg_editor_ref.js`) - fold onto `CellKey.doorKey` only if one shows
  a case bug (`17_cell.js` lowercases the name, the others do not).
- The MO2 and Morrowind.ini load orders (`layout::mo2_profile`, `GameFileN=`, `[Archives]`)
  into ordercore.
- Why `editor_records` came back unreadable on Static's setup (Messages has the first 120
  characters of the answer).
- Decide: the Object Window's search matches a record's deleting version (a later plugin's
  DELETED record). Decide: moving a response sets only its own `prev_id`/`next_id`, so two
  responses moved to the same place both claim it (the build writes them in queue order).

### The world

The region map, cell borders, the path grid's drag tools and the key binding page are in
(2026-11-02, `CHANGELOG.md`). Try them on the real setup: paint a region over a few cells
and Test in OpenMW (the region's weather and sounds there), make a row of exteriors off the
coast, a path grid joined by a box, a key moved and the controller's A moved to B.

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
- The Use Report was not in the last measured session: click it once on a warm launch and read
  Messages (the first of a launch took 22-60 s before it stopped reading every object type).
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
  the real one. Next, measure before building anything: run `probe_land_read`
  (`tests/probe_land_read.rs`, `WG_DATA` = the data folders) for the read's share, and time a
  whole Merge Lands on the real order for the rest. Only if the read is still a large share,
  keep each plugin's `LAND`/`LTEX` records on disk as the Python kept `<stem>.land.json` and
  asked the scan cache whether a plugin has terrain (`landcore` does not depend on `editcore`:
  a shared cache crate, or move the cache).
- Not yet checked for a cache lost in the port: the Lua archive scripts (the Python unpacked
  them into `wraithguard_lua_archives`; `luacore/scan.rs` says the engine has its own overlay,
  archives included: read the code, not the header) and the Conflicts window's per-scan result
  caches (the Python's `_conf_singles`, `_conflict_adjacency_cache`; the page is JavaScript
  now). Checked and kept: the NIF measurements (`nif_batch`, scan cache), the overlay's file
  index (`wraithguard_overlay_index.bin`), the conflict scan's keys, cells and terrain flags
  (`wraithguard_scan_cache.sqlite3`), the dialogue graph's lookups.

### The record cache (`editcore::cache`)

- Measure on the real 621-plugin order: the first indexing, how much of it the viewer's
  requests feel, the per-lookup `stat` of every plugin. If it is too big (the `uses` table
  is most of it): keep only the strings of fields that can name a record.
- "Search all fields": an FTS5 index of the plugins' own fields, with the pending edits
  layered over the hits.

## Opening a big install (the viewer engine)

Measured on 621 plugins: read 5 s, parse 8.7 s, merge loop 40-47 s (exteriors 26-31 s,
interiors 9-10 s, tables 3-4 s), the file index 0.7-2.1 s from its saved copy.

- Patch 0016 skips the mesh geometry read beside the merge: check the connect log says
  "meshes' geometry: not read" and the merge time. Next: exteriors (history about a third,
  the rest per-reference bookkeeping).
- A cache of the merged world only if that is not enough (a mappable flat file, invalidated
  on the whole load order).

## Release and CI

- Versions (sorted 2026-11-09 against GitHub): tags v5.0.0-v5.0.3 are pushed, and v5.0.3 is
  the only 5.x release published; `CHANGELOG.md` now has 5.0.1, 5.0.2, 5.0.3 and 5.0.4 (the
  work since v5.0.3). Before tagging 5.0.4: delete the local, never-pushed `v5.0.4` tag
  (`2bafaef`, "CodeQL fixes": `git tag -d v5.0.4`); set the version in
  `viewer-shell/Cargo.toml`, `viewer-shell/cli/Cargo.toml` and `viewer-shell/tauri.conf.json`
  (still 5.0.1: the v5.0.2 and v5.0.3 programs both call themselves 5.0.1), and build once so
  both `Cargo.lock`s carry it (`PREFLIGHT.md`, "A release").
- The MOMW Tools Pack (Static's fork, `C:\tools\build\momw-tools-pack`; `get_wraithguard`
  in `build.sh`): rewritten for 5.0.1 with its sha256s, the AppImage in the Linux pack and the
  macOS binaries as `-macos-arm64`/`-macos-amd64` (1.58, not yet sent upstream). Move it to
  the release that is current when the merge request goes in (5.0.3 now: its four archives'
  and the AppImage's sha256s from its release page), then open the merge request.

- The dependency audit (`audit.yml`; 2026-11-09's weekly run failed on `RUSTSEC-2026-0459`,
  the bundled SQLite: `rusqlite` 0.41, in 5.0.4. First run 2026-11-07: no vulnerability in any of the 15
  `Cargo.lock`, none in the page tests' npm lockfile) warns of four things in
  `viewer-shell/Cargo.lock`: `paste` 1.0.15 and `proc-macro-error` 1.0.4 (unmaintained),
  `glib` 0.18.5 (`RUSTSEC-2024-0429`, unsound; it comes in with Tauri's Linux stack, so a
  Tauri bump is the way off it), `tokio` 1.53.1 (`RUSTSEC-2026-0386`, `AsyncFd` I/O safety,
  published 2026-10-13; patched from 1.53.3: `cargo update -p tokio` took it to 1.54.1) and a
  yanked `yoke-derive` 0.8.3 (`cargo update -p yoke-derive` took it to 0.8.4; commit
  `viewer-shell/Cargo.lock`). None fails the job.
- `viewer-shell/physcore`'s tests and clippy are in neither `ci.yml` nor
  `build_and_check_rust.ps1` yet: add them to both.
- Actions are pinned to a commit of their current major; moving to the Node 24 majors
  (checkout 7, upload-artifact 7, cache 6) is a separate step: read each one's breaking
  changes, then re-pin.
- Tauri pins glib/gtk 0.18, string_cache 0.9 and jsonptr 0.7 (Aikido: waiting on upstream);
  recheck each Tauri release.
- Aikido, to ignore with these reasons: path traversal and file inclusion (a desktop app
  opening files its user picks); the two `engine_behaviour.json` "secrets" (SHA-256 hashes of
  test outputs); CSP and clickjacking on `none.com` (not our site: remove it under Domains).
  The findings about the Python (`updaters.py`, `gen_tes3_schema.py`) closed with 5.0.

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
  is animated; `ui.create`'s keys go to a node once it is clicked. Check on a real install:
  a mod's global and local scripts, OpenMW's music playlists, `I.AI` packages through
  `ai.lua`, `I.Combat` and `I.UI` from the built-ins, the OpenMW folder found at a drive's
  root (`E:/OpenMW`).
- Physics drop (`76_wg_physics.js`, `physcore`): a hollow body is split coarsely (a 32-voxel
  grid, 16 parts at most), so a thin-walled vase may still fill in. Check on a real install:
  a crate onto Balmora's tables, a book onto a shelf, rocks down a slope, a cup into a bowl.
- MWScript the preview still does not run: animations are named but not drawn (PlayGroup,
  LoopGroup: the mesh stands still), a spell's effects (Cast is counted for GetSpellEffects
  for ten seconds; GetEffect is not answered). Combat has no weapons, armor or fatigue.
  Actors move each time the scene is built again (about three times a second), so a walk
  steps rather than glides: gliding needs the renderer to move one drawn object between
  builds.
- Check on a real install: Seyda Neen's lighthouse and lamps by the hour, a mod's static swap
  by a global, a mod's music script, a ForceGreeting opening the In game tab.

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
- The checks on a real setup in `viewer-shell/GARDENFELL.md` (shadows, flipbooks, skinned
  NPCs, the UI brought in from 18hu, the Deck, Balmora's speed); the text delta
  after Robin's string audit, and the rebase onto his release after that.

## Meshes and distant view

- "Optimise draw order": compare with G7's Blender exporter's output on the same mesh.
- Distant view on a real setup: does the viewer still close with distant objects on; the
  tier bands vs the install's draw distance; the first-load time.
- From MGE XE, not taken yet: VRAM streaming with eviction, a frame-budgeted upload pump,
  horizon culling of terrain, static texture atlases, building classification.

## Controllers

`49_wg_gamepad.js`, behind Settings' "Game controller"; its buttons are remapped on the
key binding page (`69_wg_keys.js`) since 2026-11-02. Once tested on the Deck: D-pad focus
navigation for the menus; the real pointer moved where the system allows (Tauri
`set_cursor_position`).

## OpenMW Lua

- Each new OpenMW release: its engine handlers and built-in events (`luacore::api`, from the
  release's `engine_handlers.html` and `events.html`). 0.49-0.52 are there; 0.52's came from
  Static's 0.52.0 RC1 `resources` (commit 4ff261bf), so when 0.52 is released (or a later RC): its
  `API_REVISION` (166 for now, RC1's own, from its CMakeLists.txt; the Preview gives scripts
  the install's, by its `resources/version`) and its pages, for handlers and events its scripts do not
  show. Check `WG_OPENMW_RESOURCES=<resources> cargo test -p luacore an_install -- --nocapture`.

