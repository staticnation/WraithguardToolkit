# Rebasing the viewer on Gardenfell 18hu

Working notes for the rebase (R0 inventory, 2026-10-12). Our viewer was forked from
Gardenfell at about round 18ee; the common base used here is the 18ef snapshot
(`subset sort/grassforge-main`). Upstream is round 18hu (the October 2026 upload).

Ground rules (Static, 2026-10-12):

- Keep what we stripped out (grass generation, painting, the statics setup, ESP export,
  the profile/rules managers) out. Wraithguard is not a grass generator: the agreement
  with Robin is that it does not compete with Gardenfell. Brushes may come back later for
  landscape editing.
- Offer our fixes and shared rules to Robin, so upstream's crates can sit in our tree
  unmodified.
- Robin (Discord, 2026-10-12): take what is useful, but hold off on anything grass until
  Gardenfell ships as the grass generator. The Editor deleting an individual grass mesh
  is fine. He is still fixing things before release and auditing ~2000 strings of the
  page's text, so 18hu is not final: expect another rebase after his release.

## What upstream changed since our base

- The engine (`gdn_core`) is now ten crates, std only (no third-party dependencies):
  `gardenfell-bytes`, `-text`, `-tes3`, `-nif`, `-bsa`, `-install`, `-world`, `-rules`,
  `-scatter`, `-session`.
- **Its own typed readers** (18gg/18gh): `gardenfell-tes3/src/tes3/types` (49 files,
  7.8k lines, every TES3 record type) and `gardenfell-nif/src/netimmerse/types` (126
  files, 8.4k lines, every NIF block type). Their layouts are Greatness7's `tes3` crate's,
  ported to std-only Rust, and since 18gh the engine reads plugins and meshes through
  them and nothing else. No LUAL.
- `gardenfell-session` (18go, 8.5k lines): the desktop program's state and every former
  shell command.
- `openmwcfg` (18gn): the whole cfg chain, OpenMW's install cfg included, `fallback=`,
  `encoding=` (18gq code pages). Against our shared cases it passes 11 of 13: it is
  right where our cases followed OpenMW's documentation over its code (chain order,
  `replace=config`; fixed in ordercore and the cases), and it drops a quoted path with
  text after the closing quote, which OpenMW keeps (a fix to offer Robin).
- `triorder` (r37, 18ho): vertex cache and overdraw ordering, pure Rust, byte-identical
  with meshoptimizer on the corpus; ~30 % less vertex work on vanilla meshes, nothing
  visible changes (`meshopt-plan.md`). Quantization and simplification parked there.
- Performance rounds (tile cull 18hi, interiors), the page from 315 parts to 337.

## Three-way inventory (base 18ef, ours, upstream 18hu; `git merge-file`)

| | page (`ui/src`) | engine (`viewcore/src`) |
|---|---|---|
| ours only (Wraithguard) | 34 | 11 (`bcx`, `inspect`, `luascan`, `markers`, `mland`, `navmesh`, `refkey`, `review`, `usage`, both `from_crate`) |
| we never changed it: take upstream | 5 (`04_img`, `15_esp`, `28_particles`, `30_tip`, `34_outline`) | 1 (`worldmap`) |
| upstream never changed it: keep ours | - | 5 (`coverage`, `json`, `mge`, `pool`, `toml`) |
| both changed | 24 (18 with conflicts, 145 hunks) | 16 (10 with conflicts, ~50 hunks; `world.rs` 20, `nif.rs` 14) |
| base parts we stripped (stay out) | 136 | - |

Page conflicts by file: `05_text` 32, `18_cellpreview` 30, `02_body` 18, `11_events` 17,
`06_gl` 14, `09_ui` 10, then 1-4 each in `07_ini`, `10_preview`, `12_browser`, `13_mo2`,
`16_gamedata_ui`, `17_cell`, `19_settings`, `25_sky`, `29_ssao`, `31_cellmap`,
`33_fxaa`, `37_shadow`.

The engine's conflict counts understate the work: upstream's `world.rs` and `nif.rs` now
read through the typed layers above, while ours read through the `tes3` crate
(`from_crate`).

## Steps

- R0: this inventory. Done.
- R1: the engine, with **option 2** (Static, 2026-10-12): Robin's crates unmodified,
  Greatness7's `tes3` crate parses, and an adapter turns its types into Robin's (his are
  G7's layouts ported), so G7's updates arrive with a version bump and Robin's with a
  copy. Chosen over forking his world/nif because the rebase is to be offered back to
  Robin: fixes to his crates stay small patches against his files, and the adapter is a
  crate of its own he can take or ignore.
  - R1a: done. The seven non-grass crates vendored in `viewer-shell/gardenfell/`
    (`VENDOR.md`): they build; with Robin's fixture all 497 of their tests pass with our
    patches in (his 491, 6 ours); without it 32 of his cannot run.
  - R1b: the adapter crate (`viewer-shell/gardenfell-g7`).
    - Hooks in Robin's crates: `patches/0001` (world: the record typer) and `0002`
      (nif: the stream reader), defaults his own; his tests unchanged with them in.
    - Records: done. G7's `tes3::esp` types into Robin's for every kind his world types
      (header, cell, land, ltex, region, the 16 model kinds, CREA, NPC_, LEVC); other
      kinds go to his reader, as his world only reads them raw. From the raw record, what
      G7's types do not keep: optional-subrecord presence (land data, AI data, Bloodmoon
      weather), cell reference order and temporary flag, actor FLAG bits G7 does not
      know. A cell G7's map cannot hold as the file has it (a repeated reference key, an
      MVRF naming another reference) is typed by Robin's reader. Code page applied to
      G7's text. Differential test (`tests/differential.rs`): 41 plugins (our fixtures,
      Robin's page fixtures), 768 records, 0 differences. Not covered by any fixture:
      ARMO, CLOT, BOOK, ALCH, APPA, INGR, CONT, LOCK, PROB, REPA - run it with
      `GARDENFELL_G7_PLUGINS=<Data Files>` on a real install.
    - Meshes: done. G7's `tes3::nif` stream into Robin's `netimmerse::stream::Stream`,
      block for block (`src/meshes.rs` by hand: glam to arrays, matrix rows = glam
      columns, quaternion `w,x,y,z`, named enums to numbers, slot-map keys to block
      indexes, the bounding volume; `src/meshes_gen.rs` generated by
      `tools/gen_g7_nif.py` from both crates' sources: 133 structs, 7 enums, 111 of the
      112 block types, 0 unmatched fields). A file G7 refuses, or one with a block not
      converted (only the abstract `NiObject`), is read by Robin's reader. Differential
      test (`tests/differential_nif.rs`): 905 meshes (our fixture 17, Gardenfell's 120,
      NifCorpus 768): 851 the same, 0 different, 0 fallbacks, 54 refused by both. Same
      speed as Robin's reader warm (~130 ms for 138 MB of corpus).
  - R1c: done. viewcore's `json`, `pool`, `toml`, `mge`, `layout`, `plugins`,
    `weather`, `vfs`, `bsa`, `esp`, `land`, `worldmap`, `objects`, `nif` re-export
    Robin's modules plus our additions; `World` wraps his (Deref) with our extras pass
    (records index, GMSTs, greetings, pathgrids, land edits, NPCs, groundcover), and
    loads with `openmw_kinds` on (patch 0007). `viewcore::init()` installs the G7
    readers at startup (Tauri viewer and `wg-view-serve`). Both `from_crate` modules
    gone. Gate: viewcore 29 tests, clippy clean (viewcore, check-commands,
    gardenfell-g7), page boot 0 errors (16 references, 34 statics, door buttons, ten
    weathers), mesh view 0 errors, editor 12/12.
- R2: done (2026-10-12), redefined. `gardenfell-session` cannot be vendored: every one
  of its modules uses the grass crates (`gardenfell-rules`, `-scatter`) and its `App`
  holds grass state. So (Static, 2026-10-12) its non-grass changes since 18ef are ported
  into our `commands.rs`, the reply format kept as our page reads it, and Robin is asked
  whether he would split a grass-free session crate out after his release (then a later
  R2 can vendor that part unmodified, as R1 did).
  - Measured: 27 of our 51 commands are his; in most only the Tauri-to-library
    signature changed. Of his 106 session functions we lack, 95 use the grass crates
    or grass state; the other 11 are grass-tool odds and ends the viewer has no use for.
  - Ported: `open_install`'s 18fc/18gf load counts (`refsKeptDeleted`, `refsMoveDropped`,
    `refsRelocated`, `cellsDeleted`, `recordsDeletedByFlag`, `landsDefaulted`,
    `landsFlaggedOut`); `world_map`'s `sea` colour (18hp); the 18ez `still` interface -
    `mesh_data(still)`, `assets_bundle(still[])` (a static's `.kf` is not played, OpenMW's
    `useAnim()`), and `"still":true` on a reference whose class never animates, on a mesh
    with a `.kf`. All additive: our page ignores them until R3 sends `still`.
  - Not taken (grass): the preview patch's cell and invented textures filtered out of
    `cells`/`textures`; profiles, rules, statics setup, paint, export.
  - Checked: `boot.js` asserts the new keys arrive and `still` is taken; clippy clean on
    the Tauri viewer crate too.
- R3: done (2026-10-12, Static: "do it all now", text and markup included). Every part
  merged, every decision in `R3-LEDGER.md`; the engine half with it - the mesh payload's
  new trailing blocks after our gloss block (18ev glow UV sets, 18ey flipbooks with their
  frames in `assets_bundle`'s textures, 18ez billboards), the animation `xyzOrder` key,
  references to a LIGH with no mesh (18hl), the page sending `still`. Checked: the page
  guards (`tools/page_guard/`), the engine commands the page calls, all 52 shaders
  compiled with glslangValidator, boot cell / mesh / editor suites, clippy on the viewer
  crate. Three shadow fixes offered to Robin (`gardenfell/patches/page/`). Robin's string
  audit is still running: expect a text delta after his release.
- R4: everything the sandbox can run passed (2026-10-12; one stale test fixed, below).
  On Static's machine (2026-10-12): the release build runs well, the frozen load fixed by
  patch 0010. Still to tick there: the visual checks and decisions, step by step in
  `R4-CHECKLIST.md` (the sandbox has none: `R3-LEDGER.md`, "Not verified"). Found on the
  way: `test_session_script_view` stored its script with LF and so met the LF warning
  added earlier; the fixture now uses CRLF, as the game stores scripts.
- Then phase 2b/2c (references, cells) on the new engine, and the clash warnings.
