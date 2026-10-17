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

What Greatness7 said (Discord):

- The NIF fix (a simple field swap) goes into his next tes3 update. When it is out,
  drop it from our fork.
- LUAL/LUAD waits for a later breaking-changes release, because it would break every
  other tool using the library. So our fork stays for them until then. He is wary of
  them anyway: an OpenMW plugin can carry code that runs. We only parse them and carry
  them through; we never pass LUAD bytecode through unread. Keep it that way, and show
  what a LUAL/LUAD carries before it goes into a patch.

## The Editor: the plan

In the order to do them. OpenMW-CS (`openmw-master/apps/opencs`) is a reference for what
a modder expects, never for code: it is GPL-3.

### 1. Foundations

- One reader of the load order (the Rust migration), in `viewer-shell/ordercore`, which
  viewcore and native/ share. The target is what OpenMW does, never what either reader
  did. Done: openmw.cfg (phase 1); which version of a record wins and what a placed
  object is (phase 2a, `ordercore::records`). Still to do:
  - Confirm 2a in the game: `wg_clash_test/WG_Clash_A.esp` (wg_clash as a weapon) then
    `WG_Clash_B.esp` (a misc item, and the cell `WG Clash Test` placing it). Expected:
    the placed one is the weapon; `player->additem wg_clash 1` gives the weapon.
  - Phase 2b: reference identity (RefKey vs the session's `winning_reference`, `_origin`,
    `_home_cell`) and moved references (MVRF), with a shared `ref_cases.json`.
  - Phase 2c: one cell key function (interiors without case, exteriors by grid).
  - The Use Report and Verify: say when an id is defined as two object types - an error
    for the CS ("Matching IDs", it renames one) and Morrowind.exe (will not load); OpenMW
    keeps both and places one - and which one a placed object is. Tested 2026-10-12: the
    CS (CSSE) warns on `wg_clash_test`, and Morrowind.exe refuses it.
  - The MO2 and Morrowind.ini load orders (`layout::mo2_profile`, `GameFileN=`,
    `[Archives]`) into ordercore too.
- Binary data path for the Editor's lists (engine -> page), not JSON (Static, 2026-10-13:
  the Object Window sat on "0 records" with no error - `Engine.call` hands back text it
  cannot parse as a string, so a bad JSON answer looked like an empty list). Done: record
  rows as `WGOR` bytes (`editor_rows`, `pack_rows` in commands.rs; `recordRows` /
  `unpackRows` on the page) for the Object Window, the actor list, the place and record
  views, with `editor_records` JSON only as the fallback, said in Messages; placed counts
  counted once per load across the pool; lists asked again on every connect. Still to do:
  - The same for `editor_tags`, `editor_cell_refs` and `cells` (the Cell View), then the
    other big lists (`textures`, `assets`, `texture_usage`, `scopes`).
  - Done too: `Engine.call` throws when a JSON command's answer does not parse (command
    name and the first 120 characters); `Engine.TEXT` lists the two that may hand back
    text (`viewer_profile_read`, `wg_post`). `pack_rows` and `unpackRows` share a fixture
    (`ui/tests/editor/wgor_rows.json`; `wgor_tests` in commands.rs, `wgor_rows.test.js`).
  - Find why `editor_records` came back unreadable on Static's setup (the fallback's
    Messages line now carries the first 120 characters).

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

## Record cache (SQLite: `wraithguard/patch/recordcache.py`)

The editor keeps what it reads from the plugins in `wraithguard_record_cache.sqlite3`
(records by plugin and tag, a key index, the moved-reference index, use reports), valid
while a plugin's size and time are unchanged and the native reader is the build that made
it. What is done, and what is left, in the order to do it:

**Done, to move to the CHANGELOG with the next release** (the first three are already there):
the parser guard, dialogue, plugin headers/masters; the key index (which plugins have a
cell, an id, a square's land) with background and parallel indexing; the Use Report kept
under a signature of every plugin's size and time; Verify's id lookups from the key index
(one native pass per plugin indexes every tag Verify asks about - `INDEXED_TAGS` - in the
background and again before a Verify, so the first Verify is not a read of every plugin per
tag); a size limit (record bodies of the largest tag go first, the keys stay); two caches
on one file tested.

**Open:**
- Use Report, first click per record: a repeat click is a query, the first still scans every
  plugin. A per-plugin table of which ids each record mentions (record type, key, id, path)
  would make the first click a query too. The biggest item left in the editor.
- "Search all fields": an FTS5 index of the plugins' own fields, with the patch's pending
  edits layered over the hits at query time.
- Load-order tooling outside the editor (per-plugin headers, record counts, conflict sets
  for the Patch Builder and the conflict viewer): read that code first, probably a large
  win, the same hundreds of plugins.
- The viewer engine (Rust): the world's read (5 s), parse (8.7 s) and merge (54 s) and the
  205,535-file index (4 s) in a 621-plugin load order. The biggest time and the riskiest:
  a stale cache there draws a wrong scene, so it needs invalidation on the whole load
  order, not per plugin.
- The size limit drops bodies but the file does not shrink (SQLite keeps freed pages for
  reuse): a `VACUUM` when more than half the file is free, off the request threads.
- Measure on the real 621-plugin load order: the first indexing, how much of it the viewer's
  requests feel, the file's size, and the cost of the per-lookup `stat` of every plugin (a
  memo was not added: it is hundreds of calls per lookup, which should be milliseconds).

**Decided, not changed:** a record a plugin deletes still counts as present in the key index
(a name another plugin deleted is refused, not reused) - the safe direction; `find` still
sees the deletion. Not worth caching: the queue and journal (persisted already), the pending
list, search results, Verify's findings - they change with every edit.

## MWScript compiler: TESCS and MWEdit differences

**Wait for G7 first:** Greatness7's compiler goes into the tes3 crate, and he has
tested it on a community dump of 15k+ mods. When it lands we drop our MWEdit port
(`native/src/mwscript`) and use his. Before that, run both on
`MwEditScriptTest.esp` and keep the port's tests as a cross-check through the switch.
Until then, no compiler work. His policy is to reject scripts that would not
round-trip, and make the author rewrite the odd syntax TESCS accepted, rather than
match TESCS 1:1.

What matters for us whichever compiler we use: PT, TR and OAAB still maintain their
mods in the vanilla CS. So a script made in Wraithguard has to survive being recompiled
there, and a script Wraithguard recompiles must not quietly change how an old mod behaves
(Hemaris: bug reports the author cannot reproduce). So:

- Warn in Script Edit when a script would not round-trip through TESCS, and say why.
- Mark the scripts the patch recompiled, in the Build report and the plugin's
  description, so a bug report can say so.
- Never recompile a script the patch did not change.

Quirks from that thread (each wants a test once G7's compiler is in):

- **TESCS resolves an id against every script's local variables first.** It searches
  all scripts' locals, not only this script's. `GetSoundPlaying "Drink"` miscompiles
  because the `bittercup` script has a local named `drink`. So a local variable named
  like an object, sound and so on can break unrelated scripts. Check: warn on locals that
  share a name with any record id of the load order.
- **Line endings:** OpenMW-CS saved scripts with LF, not CRLF. A line ending in a
  semicolon then "commented out" the next line in the game, and scripts broke. G7 found
  missing-CRLF scripts in the community dump. We now store every script text and result
  script as CRLF (`editor.crlf`); Script Edit warns on a plugin's script with bare LFs,
  and the dialogue window on a response's result script (2026-10-13).
- **Commas:** a comma in the wrong place works in other editors (OpenMW-CS included),
  and breaks silently and mysteriously in the CS.

Then, a corpus of scripts where TESCS and MWEdit disagree, in the test harness with the
bytecode each produces (Hemaris: some scripts only work compiled by one or the other;
ask on the MMC, abot would know). Each needs a real script and TESCS's output first.

What simania.nl/mw/morromod.htm (read in full, "Syntax" and "Script quirks") reports.
TESCS lets these compile, and the game then misbehaves or fails. Most belong in our
checker (`mwscript/check`) as warnings, whatever the compiler does. Done 2026-10-13 (the
ones a line can show): the single `=`, `<>`, the stray `=` after `to`,
`<id>->GetDeadCount`, the unquoted hyphen/apostrophe/leading-digit id, the 20-`elseif`
chain; a stray `elseif` after `endif` was already caught (`'elseif' without an 'if'`).
Still to do:

- `set x to x + 1` with no parentheses (MWEdit refuses it; "the game doesn't seem to
  mind").
- A local variable another script declares but this one does not: no error, and the
  script malfunctions.
- `SetHealth`/`Mod*` given a remote variable (`mycompanion.basehealth`), or the
  `mycompanion.` qualifier left out: compiles, and fails in game (LeftEval).
- TESCS only: `StopScript ItsOwnName` in a new script needs two saves, because the
  compiler sees the saved plugin, not the session. The same happens for any record made
  in the same session. Ours must resolve against the pool.
- MWEdit only (beta): it misses `mycompanion.RemoteLocalVar` in targeted scripts,
  refuses region names (`"Ascadian Isles Region"`) with `PositionCell`, and refuses
  anything under a `Choice` line in a result field.

The "newline/whitespace" difference in the summary I was given is not on that page, so
it is unconfirmed.

## Gardenfell 18hu rebase (viewer-shell/REBASE-18hu.md)

R1 (engine, records and meshes through `gardenfell-g7`) is done. Left:

- Send Robin the patch offering (`Gardenfell-offering-2026-10-12.zip` beside the repo).
  His `fixture/` folder came: keep it out of the repo (real `Morrowind.ini` copies under
  `ru/`); put it in `viewer-shell/gardenfell/fixture` locally to run his whole suite.
- Tell Greatness7: his crate refuses an NPC_ with a FILL subrecord that Robin's reads;
  one corpus mesh panics inside `bytemuck` (caught, falls back).
- Run `gardenfell-g7`'s differential tests on a full install
  (`GARDENFELL_G7_PLUGINS`, `GARDENFELL_G7_NIFS`): ARMO, CLOT, BOOK, ALCH, APPA, INGR,
  CONT, LOCK, PROB, REPA have no fixture.
- One cfg reader: luascan, navmesh and `native/` still read openmw.cfg through ordercore.
- Phase 2b (references, MVRF) and 2c (cell keys) on the new engine; the Verify and Use
  Report clash warnings.
- R2 and R3 are done (`viewer-shell/R3-LEDGER.md`), and Static's five decisions are
  answered and in (2026-10-12: post effects on unless the install says, Robin's pill
  bar, side panel, find bar and window memory, the Editor toolbar's window docks).
  R4's release build runs well on Static's machine (patch 0010). Left: the visual checks
  in `viewer-shell/R4-CHECKLIST.md` (sections 2 and 3: window restore, the pills and the
  toolbar bar are untested outside the sandbox); the text delta after Robin's string
  audit, and the rebase onto his release after that.

## Mesh optimisation

Checked (Robin thinks there is useful stuff here too). All MIT, so fine for `native/` and
`viewcore`:

- **meshoptimizer** (zeux, C/C++, MIT): https://github.com/zeux/meshoptimizer. It covers
  indexing, vertex cache, overdraw and vertex fetch optimisation, quantisation,
  simplification (basic, attribute-aware, permissive, with vertex update), efficiency
  analysers, meshlets, compression and strips.
- **`meshopt` crate** (MIT OR Apache-2.0, newest 0.6.2, Oct 2025; bundles a pre-1.0
  meshoptimizer and builds the C++ with `cc`): https://crates.io/crates/meshopt.
  (G7's bindings crate is the one at 0.7.0; it uses `meshopt` 0.4.1.)
- Not this: PyPI's `optimesh` is an unrelated, proprietary mesh-smoothing tool.
- **Greatness7's `meshoptimizer_bindings`**: a PyO3 crate over `meshopt` 0.4.1 for the
  Blender Morrowind plugin. Its whole pipeline is `optimizeVertexCache`, then
  `optimizeOverdraw` (threshold 1.05), then `optimizeVertexFetchRemap` and
  `remapIndexBuffer`, returning the vertex remap and the new triangles. Doing the same
  on NIFs puts us in step with how G7's Blender exporter writes meshes.
- **optimesh** (marcogomez, MIT, Rust 1.87): a pure-Rust, no-`unsafe` port of
  meshoptimizer v1.1 (bit-exact against the C++ in differential tests), and a gltfpack
  port; it builds for wasm32. This is not a smoothing library: it is the same algorithms
  without a C++ toolchain, and could even run in the page. It is two stars and one
  commit (July 2026), so it is young; the `meshopt` crate is the mature choice.
- npm `meshoptimizer` (meshoptimizer.js): the official JS/wasm build of some
  algorithms, for the page.

Already in the tree with the 18hu rebase: Robin's `triorder` (round 18ho,
`viewer-shell/gardenfell/crates/gardenfell-nif/src/triorder.rs`), a pure-Rust,
line-for-line port of meshoptimizer 1.3's `optimizeVertexCache` (byte-identical with the
C++) and `optimizeOverdraw` (threshold 1.05, as G7's), plus `analyzeVertexCache`. The
viewer runs both on every drawn part as it loads (`reorder_triangles`: triangle order
only, vertices untouched), about 30 % less vertex work on vanilla meshes. So drawing
needs nothing more, and the first two steps of G7's pipeline exist without a C++
dependency. Its public functions can be called as they are (no patch to Robin's crate);
`native/` would need `gardenfell-nif` as a path dependency to use them.

Done (2026-10-13): `viewer-shell/meshopt-port`, ours, std only, offered to Robin. Ported
line for line from meshoptimizer 1.3: `optimizeVertexFetchRemap`, `optimizeVertexFetch`,
`remapIndexBuffer`, `remapVertexBuffer` (generic, one array at a time), and the
`analyzeVertexFetch`, `analyzeOverdraw` and `analyzeCoverage` analysers. Tested against
the C++ itself (`tools/meshopt_oracle/run.sh` builds it from its repository and pins its
answers in `tests/oracle_values.tsv`): 27 test meshes and all 4,431 NifCorpus parts,
every value bit for bit. In CI and `build_and_check_rust.ps1`.

Plan:

1. Done (2026-10-13): the mesh viewer's Draw order table (`native/src/nif_order.rs`
   `report`, sent in `inspect.draw_order`): cache, fetch and overdraw per shape, as the
   file has them and after.
2. Done (2026-10-13): "Optimise draw order", the `{"op": "optimize"}` edit, written
   through the tes3 crate; only shapes it improves. Corpus: 4,432 shapes the same after,
   2,150 reordered, 50 files the crate cannot read back. Still to do: compare with G7's
   exporter's output on the same mesh. As it was planned: the triangle order from
   `triorder`, then G7's third step (`meshopt-port`'s fetch remap), remapping every per-vertex array
   (positions, normals, UVs, colours, skin weights and bone indices, morph targets) with
   the same remap, written through the tes3 crate. Compare with G7's exporter on the
   same mesh. Worth it mainly for meshes leaving the viewer (the game's own draw order),
   since the viewer already reorders on load.
3. The simplifier is ported (2026-10-13, round 6): all of meshoptimizer 1.3's
   `simplifier.cpp` in `meshopt-port::simplify`, bit for bit with the C++ on the test
   set and all 4,431 NifCorpus parts (39 values each: four option sets, attributes with
   locks/protect/priority, with update, sloppy, prune, points, scale). Wired into distant
   objects as described below. Texture-aware weighting after Zhao, Jiang & Guo (ISPRS
   2022, "QEM simplification ... based on 'local-vertex' texture features"): measure the
   texture detail under each vertex's UVs (the DDS around them) with its curvature, and
   mark the busiest vertices `VERTEX_PRIORITY` or raise their UV weight - ours, a layer
   over the port, not a change to it. Static (2026-10-13): as an alternative simplify
   mode with a toggle (plain quadric error vs texture-aware), and see about LOD for the
   cells outside our loaded rings (distant objects drawn simplified instead of not at
   all). Plan (Static, 2026-10-13): step 1 distant land - done (`viewcore::distant`,
   `distant_land`, `57_wg_distant.js`), with the water carried out over it; step 2
   distant objects - done (`distant_objects`, `__lod/` meshes, `viewcore::lod`,
   `58_wg_distant_objects.js`), with step 3 inside it, the simplify switch (plain or
   texture-aware).

   Distant view, where it stands (2026-10-13) and what is left:
   - Budget now follows MGE XE (G7's Rust rewrite, GPL-2: approach and default numbers
     only, engine stays MIT): size tiers near/far/very far by radius x scale (600/800
     units), drawn to 2/5, 4/5 and all of the cells past the rings; one simplified copy per
     model (error-driven: 1%/2%/4% by the model's tier, vertices welded, normals 0.5, UVs by
     density, colours 0.5 or 0 when uniform, borders locked). Safety caps: 30,000
     instances, 1,500 models, objects no further than 10 rings past the loaded ones, far
     textures at 128 px (`__far/` names).
   - The viewer closed itself once with distant objects on (before the budget): most
     likely out of memory (WinError 10055 in Wraithguard at the same time). Retest with
     the budget; if it still closes, with Distant objects off to tell land from objects.
   - Not yet tried on a real setup: the tier bands vs the install's draw distance, how far
     the texture-aware mode's choice shows, the first-load time (model radii read once per
     model, palette from 64 px mips).
   - From MGE XE, not taken yet: VRAM streaming with eviction (WebGL has no memory budget;
     a fixed cap could stand in), a frame-budgeted upload pump (if adding the objects
     hitches), horizon culling of terrain, static texture atlases, building classification
     (MGE doubles a building's radius; needs classifier rules).
   - The rings selector is back in Preview, Cell ("Neighbour cells") as well as its pill.
   - Distant land and objects start only after the cell's objects are placed (they had
     held a load up at 84 of 92 meshes).
   Distant LOD simplification is wired in (attribute-aware, keeping UV seams and
   normals). Welding also preserves detail, dark, decal and glow UV seams and vertex
   alpha. Still to do: checks for degenerate and duplicate triangles in the asset checks.
   Robin parked quantisation and simplification (his `meshopt-plan.md`); the port is in
   the offering to him, so whichever of us wires it in does it once.

Going with Robin's way (Static, 2026-10-12) for what `triorder` lacks (vertex fetch, the
other analysers, simplification): port those few functions line for line beside his,
with meshoptimizer's own analysers as the test oracle; no `meshopt` or `optimesh`
dependency. Measure on `NifCorpus/` as with his.

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
