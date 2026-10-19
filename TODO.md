# To do

Open work only. Done items go to `CHANGELOG.md`; what a finished item taught us goes
beside its code, or in `viewer-shell/gardenfell/VENDOR.md` for the vendored crates.

## The road to one program

Static's plan (2026-10-16/17): land editing, then the Editor's fine details, then the Python
GUI moved into the viewer, so everything is Rust and the page, with no Python, in one
program, and a clearer UI on the way. In order:

1. **Close what is open now.** Measure the merge after patch 0014, then exteriors (below);
   the Aikido leftovers; confirm phase 2a in the game; the R4 visual checks; send Robin
   0011-0014.
2. **Land editing** ("The world": raise, lower, smooth, flatten; textures and vertex colours).
3. **The Editor's fine details** (Foundations 2b and 2c, the two-types warning, the MO2 and
   Morrowind.ini orders into ordercore, `editor_records` unreadable, the region map, cell
   borders, path grid tools, shortcuts).
4. **The port, backend first**, so each step ships on its own and Python only shrinks:
   1. *The Editor's calls into Python.* The page reaches Wraithguard through 16 links
      (`links()`: record, set-many, refs-set, undo, redo, revision, columns, search-all,
      new cell, new land, verify, test setups, test run, build, review, poll), served by
      `gui/editorlink.py` over `wraithguard/patch` (14k lines) and its SQLite record cache.
      Each becomes a Tauri command; `recordcache.py` becomes Rust (rusqlite), and its
      pickled rows a plain format. After this the Editor runs without the Python app.
   2. *Load order.* Sort (`rules/` for mlox, MOMW's `plugin-order.yml`, `sort/`), Export
      (openmw.cfg, customizations.toml), the rule panels, the rule maker, Update rules and
      Sources (`net/updaters.py`), into ordercore and a Load order page. Today's main
      window, so the biggest UI piece.
   3. *Conflicts.* Record conflicts (`gui/conflicts.py`, 3.8k lines), resource conflicts,
      the cell map (the viewer's coverage map already does most; `viz/cellmap.py` retires).
   4. *Tools.* Merge Lands (`land/`; `native/src/land.rs` is most of it), Merge Settings,
      Lint (`native/src/lint.rs`), Save Check, tes3cmd, Remove Master, Backups, the Lua view
      (luacore has it), the journal and plugin views, the help (`viz/docs.py`).
   5. *Drop Tk and PyInstaller.* One Tauri build per platform; the Flatpak loses its Python
      runtime (and the `LD_LIBRARY_PATH` workaround); `native/` (pyo3) folds into viewcore
      and ordercore as its last Python caller goes.

   Each Tk window is redesigned as it moves, not copied: that is the UI cleanup.
5. **Later, not blocking the port:** a sound engine (plan in `viewer-shell/SOUND.md`: decode
   in Rust with Symphonia, play in the page with Web Audio; its step 1, a player for sound
   records and files, could come with step 3); the MWScript compiler work (waits
   on G7); LUAL upstream; controllers (the Deck); the MGE extras; OpenMW Lua per release;
   Teal in the viewer.

## Security and dependencies

Aikido after the 4.5.0 push (2026-10-19 rescan): 33 solved; glib, gtk, string_cache and
jsonptr ignored as Tauri-pinned (below). Left:

- **`paste`:** merge_to_master no longer uses `lending-iterator` (its one `windows_mut`
  is a `split_at_mut` loop now; done locally). Push merge_to_master, then
  `cargo update -p merge_to_master` in `native/`.
- **`assert` in `tools/gen_tes3_schema.py`:** explicit checks now (done locally).
- **To ignore, with these reasons:** path traversal and file inclusion (a desktop app
  opening the files its user picks); the two `engine_behaviour.json` "secrets" (SHA-256
  hashes of test outputs); pickle (Wraithguard's own cache file; gone with 4.1 of the port);
  SSRF in `updaters.py` (http/https only, a host required, a size cap, hostile URLs
  tested); CSP and clickjacking on `none.com` (not our site: remove it under Domains).
- **`innerHTML` (40 places in the page):** plugin text is untrusted (a mod can put markup in
  a name), so check each place escapes what it inserts (`escHtml`). Then a CSP
  (`tauri.conf.json` has `"csp": null`), which first needs the page's ~230 inline `on...=`
  handlers moved to `addEventListener`: do it page by page with the port.
- **Tauri 2:** glib/gtk 0.18 (Linux GTK3, unmaintained), string_cache 0.9 (html5ever) and
  jsonptr 0.7 (json-patch) are pinned by Tauri; even 2.12.4 keeps them. Recheck on each
  Tauri release. In Aikido: "waiting on upstream", and the two `engine_behaviour.json`
  "secrets" are SHA-256 hashes of test outputs (false positives).
- CI: every action is pinned to a commit of its current major (2026-10-19); moving to the
  Node 24 majors (checkout 7, setup-python 7, upload-artifact 7, cache 6, ...) is a
  separate step: read each one's breaking changes, then re-pin. `ubuntu-latest` becomes Ubuntu 26 on
  2026-10-19: watch the Linux and Flatpak builds.

## Forks: LUAL, Clippy

tes3 and merge_to_master come from `staticnation/tes3` and `staticnation/merge_to_master`
(the `[patch]` sections in native/, viewer-shell/, viewcore/ and check-commands'
Cargo.toml).

- Offer merge_to_master's LUAL work upstream (a PR to Greatness7/merge_to_master, once
  tes3#7 is merged there), and ask greatness7 for the `ignore/` pairs behind the `MW`,
  `MW_TB`, `MW_BM` and `MW_TB_BM` dialogue tests.
- When both are upstream: delete the `[patch]` sections and point merge_to_master back
  at Greatness7.
- Greatness7: the NIF field-swap fix goes into his next tes3 update - drop it from our
  fork then. LUAL/LUAD waits for his next breaking release. We only parse them and carry
  them through, never LUAD bytecode unread; show what a LUAL/LUAD carries before it goes
  into a patch.

## The Editor

OpenMW-CS (`openmw-master/apps/opencs`) is a reference for what a modder expects, never
for code: it is GPL-3.

### Foundations (`viewer-shell/ordercore`, one reader of the load order)

- Confirm phase 2a in the game: `wg_clash_test/WG_Clash_A.esp` (wg_clash as a weapon) then
  `WG_Clash_B.esp` (a misc item, and the cell `WG Clash Test` placing it). Expected: the
  placed one is the weapon; `player->additem wg_clash 1` gives the weapon.
- Phase 2b: reference identity (RefKey vs the session's `winning_reference`, `_origin`,
  `_home_cell`) and moved references (MVRF), with a shared `ref_cases.json`.
- Phase 2c: one cell key function (interiors without case, exteriors by grid).
- The Use Report and Verify: say when an id is defined as two object types (the CS warns
  "Matching IDs", Morrowind.exe refuses it; OpenMW keeps both and places one), and which
  one a placed object is.
- The MO2 and Morrowind.ini load orders (`layout::mo2_profile`, `GameFileN=`,
  `[Archives]`) into ordercore too.
- Find why `editor_records` came back unreadable on Static's setup (the fallback's Messages
  line carries the first 120 characters of the answer).

### The world

- Terrain editing: raise, lower, smooth and flatten with brushes; land textures and vertex
  colours painted (OpenMW-CS's three terrain modes). Merged Lands' land paths are the start;
  new land of the patch's own (flat, at the sea) is in.
- A region map: cells coloured by region, regions painted onto cells, exteriors made by
  dragging - from the cell map.
- Cell borders and names drawn in the render window beside the cell arrows.
- Path grid tools as OpenMW-CS has them: a connection dragged out from a point, points
  joined by a box.
- Remappable Editor shortcuts (a key binding page), with the controller bindings below.

## Record cache (`wraithguard/patch/recordcache.py`; moves to Rust in step 4.1)

- If the cache's 356-420 MB is too much (the `uses` table is most of it): keep only the
  strings of fields that can name a record. If the first indexing is too slow: more
  workers (memory: one whole decoded plugin each).
- Measure on the real 621-plugin load order: the first indexing, how much of it the
  viewer's requests feel, and the per-lookup `stat` of every plugin.
- "Search all fields": an FTS5 index of the plugins' own fields, with the patch's pending
  edits layered over the hits at query time.
- Load-order tooling outside the editor (per-plugin headers, record counts, conflict sets
  for the Patch Builder and the conflict viewer): read that code first; probably a large
  win on the same hundreds of plugins.

Decided: a record a plugin deletes still counts as present in the key index (a name
another plugin deleted is refused, not reused). Not cached: the queue and journal, the
pending list, search results, Verify's findings.

## Opening a big install (the viewer engine)

Measured on 621 plugins: read 5 s, parse 8.7 s, merge loop 51.8 s (the mesh read beside
it waits 19 ms), the 205,535-file index 4 s; about 6 us a reference across 8.6M.

- The merge by part (Static, 2026-10-19; loop 39-59 s once warm): exteriors 17.8-27.4 s
  (history in it 6.2-9.9 s, sampled), definitions 11.9-13.8 s, interiors 5.7-8.8 s,
  tables 2.3-10.3 s, the rest under 0.5 s. Definitions were nearly all the twins' `.kf`
  reads (9.5-11.4 s for 614 files, one by one) and twin lookups (2.1 s): now asked ahead
  on the pool (patch 0014) - check `x twins answered before the loop` and that
  definitions dropped. Next: exteriors (the history is a third of it; the rest is the
  per-reference bookkeeping) and tables (`record_where`, one entry per id-bearing record).
- The file index is reused: all 1233 folders from the saved index, 0.7-2.1 s (checking
  the folders' times). Was 4.2 s.
- A cache of the merged world would need invalidation on the whole load order and a
  mappable flat file, not SQLite; only if the above is not enough.

## MWScript compiler: TESCS and MWEdit differences

**Wait for G7:** his compiler goes into the tes3 crate (tested on 15k+ mods); then we drop
our MWEdit port (`native/src/mwscript`), after running both on `MwEditScriptTest.esp` and
keeping the port's tests as a cross-check. No compiler work until then. PT, TR and OAAB
still use the vanilla CS, so:

- Warn in Script Edit when a script would not round-trip through TESCS, and say why.
- Mark the scripts the patch recompiled, in the Build report and the plugin's description.
- Never recompile a script the patch did not change.
- Tests once G7's compiler is in: TESCS resolves an id against every script's locals first
  (`GetSoundPlaying "Drink"` vs `bittercup`'s `drink`; warn on locals named like a record
  id); a comma in the wrong place breaks silently in the CS.
- A corpus of scripts where TESCS and MWEdit disagree, with each one's bytecode (ask on
  the MMC; abot would know).
- Checker warnings still to do (morromod.htm): `set x to x + 1` without parentheses; a
  local another script declares but this one does not; `SetHealth`/`Mod*` given a remote
  variable or missing its `mycompanion.` qualifier; `StopScript ItsOwnName` / records made
  in the same session (resolve against the pool); MWEdit's misses (`RemoteLocalVar` in
  targeted scripts, region names with `PositionCell`, anything under `Choice` in a result).

## Gardenfell 18hu rebase (viewer-shell/REBASE-18hu.md)

- Send Robin the patch offering (`Gardenfell-offering-2026-10-12.zip`, with 0011 and
  meshopt-port; 0012-0014 are in `NOTES-FOR-ROBIN.md` too, rebuild the zip). His `fixture/` stays out of the repo (real `Morrowind.ini` copies under
  `ru/`); put it in `viewer-shell/gardenfell/fixture` locally to run his suite.
- Tell Greatness7: his crate refuses an NPC_ with a FILL subrecord that Robin's reads; one
  corpus mesh panics inside `bytemuck` (caught, falls back).
- Run `gardenfell-g7`'s differential tests on a full install (`GARDENFELL_G7_PLUGINS`,
  `GARDENFELL_G7_NIFS`): ARMO, CLOT, BOOK, ALCH, APPA, INGR, CONT, LOCK, PROB, REPA have no
  fixture.
- One cfg reader: luascan, navmesh and `native/` still read openmw.cfg through ordercore.
- R4: the visual checks in `viewer-shell/R4-CHECKLIST.md` sections 2 and 3 (window
  restore, the pills and the toolbar bar); the text delta after Robin's string audit, and
  the rebase onto his release after that.

## Meshes and distant view

- "Optimise draw order": compare with G7's Blender exporter's output on the same mesh.
- Asset checks: degenerate and duplicate triangles.
- Distant view on a real setup: does the viewer still close with distant objects on (if so,
  retest with them off); the tier bands vs the install's draw distance; how far the
  texture-aware mode's choice shows; the first-load time.
- From MGE XE, not taken yet: VRAM streaming with eviction (a fixed cap could stand in), a
  frame-budgeted upload pump, horizon culling of terrain, static texture atlases, building
  classification (MGE doubles a building's radius).

## Controllers

`49_wg_gamepad.js`, behind Settings' "Game controller". Once tested on the Deck and with
Robin: an action layer and a binding table so buttons can be remapped (inputs write
impulses, the camera consumes them once a tick); D-pad focus navigation for the menus;
move the real pointer where the system allows (Tauri `set_cursor_position`: Windows, X11).

## OpenMW Lua tools

- Each new OpenMW release: add its engine handlers and built-in events table (`lua/api.py`,
  from the release's `engine_handlers.html` and `events.html`). 0.49-0.51 are there.
- The Teal checks in the viewer too (htl in viewcore: a vendored Lua 5.4 in the viewer
  binary, so measure first).
