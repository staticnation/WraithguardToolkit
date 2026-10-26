# Later, and the gated items (plan)

Step 5 of the road in `TODO.md` ("later, not blocking the port") plus the sections that wait
on something outside this repo or on measuring. These are not slices; each has a gate and a
first step for when the gate opens.

Draft from `TODO.md` only; I did not read `SOUND.md` or the code behind these.

## Gated on someone else

- **MWScript compiler** (waits on G7): his compiler goes into the tes3 crate. Gate: it lands
  there. First step: run it and our MWEdit port (`viewer-shell/editcore/src/mwscript`) on
  `MwEditScriptTest.esp`, keep the port's tests as a cross-check, then drop the port. Then
  the TESCS round-trip work: warn in Script Edit when a script would not round-trip, mark
  recompiled scripts in the Build report and the plugin description, never recompile an
  unchanged script, the checker warnings listed in `TODO.md`, and a corpus where TESCS and
  MWEdit disagree (ask on the MMC). No compiler work before the gate.
- **LUAL upstream**: gate is tes3#7 merged into Greatness7's repo. First step: offer
  merge_to_master's LUAL work as a PR and ask for the `ignore/` pairs behind the `MW`,
  `MW_TB`, `MW_BM`, `MW_TB_BM` dialogue tests. Then delete the `[patch]` sections (native/,
  viewer-shell/, viewcore/, check-commands) and point merge_to_master back at him. His NIF
  field-swap fix: drop ours when his next tes3 update has it. LUAL/LUAD waits for his next
  breaking release; we only parse and carry them, and show what one carries before it goes
  into a patch.
- **Robin's Gardenfell 18hu rebase**: send the offering zip; tell Greatness7 about the NPC_
  FILL refusal and the `bytemuck` panic; run `gardenfell-g7`'s differential tests on a full
  install (`GARDENFELL_G7_PLUGINS`, `GARDENFELL_G7_NIFS`; ARMO, CLOT, BOOK, ALCH, APPA, INGR,
  CONT, LOCK, PROB, REPA have no fixture); one cfg reader for luascan, navmesh and `native/`.
  Several of these are messages, not code.

## Gated on a new release

- **OpenMW Lua per release**: the API tables (`wraithguard/lua/openmw_api.py`,
  `luacore/src/api.rs`) are regenerated when OpenMW ships. First step is a generator you can
  rerun and diff, so a release is a review rather than a rewrite. Check whether one exists.

## Features with no gate (pick when you want them)

- **A sound engine** (`viewer-shell/SOUND.md`): decode in Rust with Symphonia, play in the
  page with Web Audio. Its step 1, a player for sound records and files, could ship with
  4.3's Editor pages as `TODO.md` says.
- **Teal in the viewer** (`luacore/src/teal.rs` exists; the page side is what is left,
  I believe: check).
- **Controllers (the Deck)** with the shortcuts page of `EDITOR-WORLD.md`, which should be
  built to take them.
- **The MGE extras** (`viewcore/src/mge.rs` exists): scope not stated in the TODO; ask
  yourself what is left before sizing it.

## Performance, only if measured

- **The record cache**: if 356-420 MB is too much, keep only the strings of fields that can
  name a record (the `uses` table is most of it); if the first indexing is too slow, more
  workers (a whole decoded plugin each). Measure on the 621-plugin order first: the first
  indexing, how much of it the viewer feels, and the per-lookup `stat` of every plugin.
  "Search all fields" is an FTS5 index with the pending edits layered over the hits.
- **Opening a big install**: the merge loop (40-47 s; exteriors 26-31 s, interiors 9-10 s,
  tables 3-4 s) is the cost. Next is exteriors: history is about a third, the rest per-
  reference bookkeeping. A cache of the merged world needs invalidation on the whole load
  order and a mappable flat file, not SQLite; only if the above is not enough.

## Security, in step with the port

The `innerHTML` audit, the Tauri-pinned crates and the CSP are in `DROP-TK.md` (slices 7 and
9), because the CSP needs the inline handlers gone page by page. The Aikido ignores
(path traversal, `engine_behaviour.json` hashes, SSRF in `updaters.py`, `none.com`) are
your clicks in Aikido, not code.
