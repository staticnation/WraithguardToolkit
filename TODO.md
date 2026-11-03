# To do

Open work only. Done items go to `CHANGELOG.md`; what a finished item taught us goes
beside its code, or in `viewer-shell/gardenfell/VENDOR.md` for the vendored crates.
Cleaned up 2026-11-02: the port to one program is done (5.0.0, `viewer-shell/DROP-TK.md`),
so its history left this file.

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

## After 5.0 (`viewer-shell/DROP-TK.md`)

- Translations: the page's string tables generated from the `.po` files (English only now).
- The Flatpak rebuilt on the Tauri binary (`build-flatpak.yml` still packages 4.x).
- A CSP: `tauri.conf.json` has `"csp": null`; the page's inline `on...=` handlers move to
  `addEventListener` first, then the `WG_TAINT` runs once more, then mark Aikido's finding.
- Android and iOS (DROP-TK slice 10: an audit of what assumes a desktop first).
- An installer; the viewer wearing the Tk settings' `theme` when its profile names none;
  Lint on the scan cache.

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

- Push merge_to_master's `lending-iterator` removal (Aikido's `paste`), then
  `cargo update -p merge_to_master` in `viewer-shell`.
- Offer merge_to_master's LUAL work upstream (once tes3#7 is merged), and ask greatness7 for
  the `ignore/` pairs behind the `MW`, `MW_TB`, `MW_BM` and `MW_TB_BM` dialogue tests.
- When both are upstream: delete the `[patch]` sections.
- Greatness7: the NIF field-swap fix goes into his next tes3 update (drop it from our fork
  then); LUAL/LUAD waits for his next breaking release. Show what a LUAL/LUAD carries before
  it goes into a patch.

## Scripts in the scene (`73_wg_scripts.js`)

- OpenMW Lua (`viewcore::luarun`, `74_wg_lua.js`): `nearby.castRay` from the engine's
  collision shapes (it finds nothing yet); `onNewExterior` (OpenMW's fish spawning) and
  other built-ins left out; `ui.create` windows. Check on a real install: a mod's global and
  local scripts, OpenMW's music playlists, `I.AI` packages through `ai.lua`.
- Physics when placing objects (optional), with Bullet - the engine OpenMW itself uses
  (bullet3, zlib license; `subset sort\bullet3-master`):
  - A `physcore` crate: BulletCollision, BulletDynamics and LinearMath built with `cc`, behind
    a small C wrapper (a world; static triangle meshes; dynamic convex bodies; step; read a
    body's transform). Rust calls only the wrapper.
  - Static: the scene's collision shapes (the engine's `collision_bundle`, as the walk body
    uses) and the land's heights. Dynamic: the objects being placed, as convex hulls of
    their collision shapes (their visual mesh where they have none), at their scale.
  - In the Editor: a "Physics" toggle beside Drop. Selected objects fall, tumble and settle
    (gravity, friction, a time limit, a rest threshold); the run is shown as it happens,
    then Keep or Drop, as the Land view's preview. Keep writes the resting places as one
    ordinary move edit (undoable).
  - Settings: gravity, friction, bounce, the most seconds to simulate, and "only up to the
    ground" (no tumbling: the drop alone).
- Dialogue window, Map tab: zoom (wheel), pan (drag), Fit, and a larger area. Rings spaced
  by how many topics they hold, so 2-step maps stop overlapping. Full labels once zoomed in.
- Dialogue window, In game tab: conditions that can be set, so fewer are only "assumed":
  - the cell from a picker of the load order's cells, with NotCell checked too (it is only
    assumed now, even with a cell given);
  - the player's sex, race, faction and rank, and level;
  - the speaker's rank;
  - the hour and the day;
  - globals and journal indexes, shared with the Scripts panel's game state.
- MWScript the preview still does not run (logged once each): choosing and showing dialogue
  (ForceGreeting says who greets; `Choice` belongs to dialogue results), animations
  (PlayGroup, LoopGroup), spells and their effects. Combat is a stand-in: blows of 4-11
  health a second, no weapons, skills or animations. Actors move each time the scene is
  built again (about three times a second), so a walk steps rather than glides.
- Check on a real install: Seyda Neen's lighthouse and lamps by the hour, a mod's static swap
  by a global, a mod's music script.

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
  `API_REVISION` (0 until then) and its pages, for handlers and events its scripts do not
  show. Check `WG_OPENMW_RESOURCES=<resources> cargo test -p luacore an_install -- --nocapture`.

