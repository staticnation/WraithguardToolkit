# Patches to Gardenfell 18hu, for Robin

From Static (Wraithguard). Each patch is a unified diff against your 18hu source, made
from the repository root (`crates/...`), and none of them touch grass. Wraithguard carries
them on its vendored copy of your seven engine crates (bytes, text, tes3, nif, bsa,
install, world) until you take them or say no; either answer is fine, we keep the
copy otherwise unmodified so your next release drops straight in. With your fixture
folder (thank you for it) your whole suite passes with the first ten applied: 1553
pass, 0 fail, against 1541 without them; the 12 more are tests we add. 0011-0014 came
later (see "Applying them"); 0017 changes a rule of 18fc's, checked in the game (see its
follow-up below); 0018-0022 (2026-10-29/30) are the newest.

## 0001 world: a record typer hook

`gardenfell_world::world::set_record_typer(fn)`. The one line in `parse_plugin_named`
that types a record (`Object::from_raw`) goes through a hook that defaults to it, so
nothing changes unless a program sets one. Why: Wraithguard wants Greatness7's `tes3`
crate to do the reading (so his updates arrive with a version bump) while your world code
runs unchanged. Your types are his layouts, so a converter is mechanical; ours is
`gardenfell-g7`, and its differential test (G7 + converter against your reader) gives 0
differences on 768 records in 41 plugins, your `Dele*` fixtures included. Happy to share
it: it doubles as a cross-check of your port against his crate.

Differences it found between `tes3` and your port, all on your side of right: `tes3`
keeps one reference per `(mast_index, refr_index)` (a repeated key loses the earlier),
drops an `MVRF` that names another reference than the next `FRMR`, and keeps only the
`FLAG` bits it knows. Your notes already say the first two; we read those from the raw
record, as you do.

## 0002 nif: a stream reader hook

`gardenfell_nif::nif::set_stream_reader(fn)`: the same for `read_stream`'s
`Stream::parse`. Same reason, for meshes. `gardenfell-g7`'s mesh half converts G7's
`NiStream` block for block into your `Stream` (111 of your 112 block types; the abstract
`NiObject` is the one left, never a block in a file). Its differential test against
`Stream::parse`: 905 meshes (Wraithguard's fixture; your page and anim meshes from 18ef
and 18hu, 120 files; a 768-file corpus of mod meshes): 851 the same, **0 different**,
54 refused by both readers. Your port and G7's crate agree on every mesh both read.

## 0003 install: `Vfs::providers`, as the resolver looks

Every place that supplies a path, in the order your resolver looks: the first is the file
`resolve_family` loads, each one after it the one that would answer were those before it
gone. Wraithguard's viewer shows "which mod supplies this mesh/texture, and what it
overrides", and needs `Root::index`, which is private, so it can't be written outside the
crate.

Thank you for the review: the first version built its own order and was wrong in exactly
the three cases you found (one mod with both spellings, `.dds` across archives, a texture
without `textures/`). Now `resolve_family` and `providers` are one walk (`family_walk`,
with a callback, so the resolver still stops at its first hit and builds nothing), plus
`texture_providers` / `mesh_providers` that look with and without the prefix as
`resolve_texture` / `resolve_mesh` do. Whatever rules you settle on for the resolver, the
list follows them. Tests: `tests/wraithguard_providers.rs` (your three cases and meshes;
each checks the list's first is what the resolver loads, and the bytes it loads).

On your proposal for the resolver: for OpenMW we checked your column against its code
(`components/misc/resourcehelpers.cpp`, `correctResourcePath`; `components/vfs/
registerarchives.cpp`) and it holds - `.dds` across the whole data set first, then the
spelled name; no `.bmp`/`.png`/`.jpg`; archives registered before every data folder, so
loose always wins, the last-listed archive over earlier ones and the last data folder
over earlier ones. Two more steps there for your rule 4, OpenMW side:
- a path that has `textures/` (or `bookart/`) somewhere inside it keeps what follows it
  and drops what came before; only a path with neither gets `textures/` put in front;
- not found, OpenMW tries the bare file name directly under `textures/` (`.dds` first,
  then the spelled extension) before giving up.
We cannot check the Morrowind.exe column; the test kit would settle it.

## 0004 install: the ten weathers, as an opt-in (and Glare View)

`ALL_WEATHERS` (the ten) and `*_with` versions of `ini_json`, `mge_json`,
`openmw_to_ini` and `atmosphere_json` that take the list to read; your plain functions
call them with `WEATHERS`, still `["Clear"]`, so Gardenfell reads exactly what it did
and your round 17h tests stand. Plus `glareView` in each weather's JSON (an extra key).

Why: you cut the weathers to Clear in round 17h along with the precipitation, wind and
storm particles, which made sense for a grass tool. Wraithguard's viewer uses the other
nine for what they do to the sky, the fog and the water only (colour ramps, fog ratios,
Weather Adjuster presets, Glare View; still no particles), so a modder can look at a
cell under the weather the region rolls. Tests: `tests/wraithguard_weather.rs` (the plain
reader is still Clear only; the opt-in reads every block and its Glare View; openmw.cfg
fallbacks land in their own weather). Our first version simply changed `WEATHERS`, which
your weather tests caught once we had your fixture.

## 0005 bytes: openmwcfg quoted path with text after the quote

`unquote` only unquoted a value that both starts and ends with `"`, so
`data="C:\Mods\X" anything` came back with its quotes and the folder was lost. OpenMW
uses the quoted part and logs the rest (`components/files/configurationmanager.cpp`,
`operator>>` for `MaybeQuotedPath`, `std::quoted(.., '"', '&')`). Now the same.

## 0006 world: every version of a reference

`World::history.refs`: for each reference (by `RefNum`, so a moved reference keeps it),
every plugin that supplied a version, in load order, and each version's placement
(`Pose`: id, position, rotation, scale). Filled at the two places your merge stores a
reference. Wraithguard's inspector shows "created by A, moved by B, swapped by C" and
the review marks show what a plugin moved. Might be useful for your conflict display
too.

It sits beside the cells rather than on `CellRef` on purpose: your grass crates build
`CellRef` and `Cell` field by field (62 places), and a new field there would break them.
`World` is only ever built with `..Default::default()`, so the series changes no code of
yours outside `world.rs`. If you would rather have the fields on the structs, say so and
we will redo it that way.

## 0007 world: OpenMW's rule for an id of two kinds, as an option

`LoadOpts::openmw_kinds`, **off by default**, so your 18gn rule and its tests are
unchanged (all pass). We read OpenMW differently from esp-format §9.4, and the option
lets each front end pick:

- `apps/openmw/mwworld/esmstore.cpp`, `ESMStore::load` (≈ lines 450-457): each record
  goes into its own kind's store (`it->second->load(esm)`); a deleted one is
  `eraseStatic` from **that** store only. Nothing takes an id out of another kind's store.
- `ESMStore::setUp` → `rebuildIdsIndex` (≈ 534-549): the id -> kind index is rebuilt
  after loading by walking `mRecNameToStore` (a `std::map` keyed by the record-name int,
  `fourCC`, little-endian) and assigning `mIds[id] = recordType` store by store, so the
  last store in key order wins: the kind whose tag is greatest read as a little-endian
  `u32`, in effect the last letter (`WEAP` over `MISC`, `STAT` over `CREA`, `NPC_` over
  everything).
- `cellstore.cpp` (`loadRef`, ≈ 967): a placed reference's kind is `store.find(id)`, that
  index.

So in OpenMW a STAT `x` then a later CREA `x` places a static, and deleting the WEAP of a
WEAP+MISC pair leaves the MISC. §9.4's rule ("a later plugin's record replaces an earlier
one of another kind", after `merge_to_master`) is what a merge produces, and the
Construction Set refuses the pair outright ("Matching IDs", offering to rename) while
Morrowind.exe will not load it. Static has two test plugins for an in-game check
(`WG_Clash_A.esp`, `WG_Clash_B.esp`). With the option on, `placed_types` decides which
kind's definitions reach `models`/`lights`/`actors`; `PluginData::ids_deleted` and
`ModelDef::tag` are what it needs. Tests: `tests/wraithguard.rs`.

## 0008 nif: what Wraithguard's viewer draws with

Additions to `nif.rs`, and a new `nif/skeleton.rs`; nothing existing changes behaviour.

- The gloss map (`NiTexturingProperty` slot 3) on `DrawPart::gloss_tex`. Neither
  Morrowind nor OpenMW draws it; a mesh viewer's studio view can.
- A name for a texture held inside the NIF (`NiSourceTexture` with no file, pixels in
  an `NiPixelData`): a prefix and the pixel record's index, so a front end can ask for it.
- `NiVisController` visibility over a shape (`vis`: every controller on the shape or an
  ancestor, keys held, not blended).
- Bone names on `BoneAnim`, and a `keep_skin` read that keeps every skinned shape whole
  (its skin, bones and weights) for hanging on another file's skeleton.
- `skeleton.rs`: a skeleton read once with an idle bound; body parts bound to it by bone
  name, as OpenMW binds a body part to the actor's skeleton (a bone the skeleton lacks
  keeps the part's own); `KfData::window(start, stop)`, the keys one clip plays plus one
  either side. Why the last: `xbase_anim.kf` is minutes of keys for every bone, and
  carried whole into each bone of each skinned part a town's NPCs came to hundreds of
  megabytes for a few seconds of idle. Tests: `kf_window_tests`.

Useful to you if Gardenfell ever shows NPCs or creatures in the preview; otherwise we
carry it.

2026-10-14 correction to 0008: the patch added `KfData::window` but missed its call
in `bind_kf`, which Wraithguard 4.3 used. Binding a short idle therefore retained
all of the animation file's keys and copied them into each body part's bone chains.
The patch now windows the bound tracks and tests the complete skeleton read/bone
binding path with synthetic data, checking key count, boundary interpolation and
an unchanged source sequence. The customer's stalled report contains NPC batches;
this is a confirmed size regression, not proof of the stall's final cause.

Skeleton binding now removes only `PartAnim::nodes`, which skinning replaces,
and keeps the part's alpha and visibility controllers. Previously clearing the
whole `PartAnim` also removed material fades and show/hide keys. The synthetic
binding regression covers both moving and baked skins, alpha and visibility
independently and together, and unskinned parts.

## 0009 world: what each plugin did to a cell

`World::history.interiors` / `.exteriors` (`CellHistory`): `touched_by`, every plugin
with a CELL record for the cell, in load order (a reference moved in does not count),
and `gone`, the references a later plugin took out (`Gone { r, by, moved }`: the version
that was there, which plugin, and whether it was moved away rather than deleted). Filled
in the merge where it already removes them; a deleted cell forgets its history with its
references. Wraithguard's cell view says "this plugin deleted these three references";
with 0006 it is the whole history of a cell.

## 0010 bsa + install: an archive is opened, not read

`Bsa::open` reads the header and the tables and keeps the file open; `Bsa::read(key)` reads
one entry with a positioned read (`read_at` / `seek_read`, std only, so threads share the
handle) and `Vfs::load` goes through it. `get` still lends a `&[u8]`: from a file it reads
the entry the first time and keeps it, which is what your tests and examples use it for.
`Bsa::parse(path, bytes)` is unchanged, in memory.

Why: your module comment weighs reading archives whole as "the same cost the game pays",
and for the vanilla three that is half a gigabyte. A modded install lists far more -
Tamriel Data, Project Cyrodiil and the rest are gigabytes - and they were all resident at
once. Static's real setup (1233 data folders) froze in "Placing objects" with every bundle
slower than the last; measured here on an 800 MB archive, opening it went from 21 s and
804 MB resident to under a millisecond and 4 MB, reading the same entries. Gardenfell
reads the same archives, so it should help your connect too. Tests:
`tests/wraithguard_bsa.rs` (an archive built in the test, opened and parsed, every entry
the same through `read` and `get`; a damaged one refused, not panicked on).

## 0011 bsa + install: part of a file, without the rest

`Bsa::size(key)` and `Bsa::read_range(key, start, len)` (the same positioned read as
0010's `read`, inside one entry, cut at its end), and `Vfs::size(loc)` and
`Vfs::load_range(loc, start, len)` over both a loose file (seeked) and an archive entry.
Additive; nothing of yours calls them.

Why: a texture's mip levels above the Texture detail were read and then thrown away. On
Static's setup one cell read 1.2 GB of textures to put 15 MB on the GPU (4K replacers, at
a detail of 512), and that was most of the load. The viewer now reads a DDS's header,
works out where the first level it keeps starts, rewrites the header for that level and
reads from there to the end: a 4096 DXT5 at 512 is about a third of a megabyte instead of
21. The trim is ours (`viewcore::img::dds_trim`), happy to send it if you want it in your
`img`. Tests: `crates/gardenfell-bsa/tests/wraithguard_bsa.rs`.

## 0012 install: a root's index kept between connects

`Root::index_one` (one root walked exactly as `Vfs::with_roots` walks it, plus every
folder the walk read, with its modified time), `Root::from_index` (a root rebuilt from
such a list without touching the disk), `Root::index` (a root's list, to save) and
`Vfs::from_roots` (a `Vfs` from roots already built). The walk itself is unchanged; it
only also says which folders it read. Additive; nothing of yours calls them.

Why: on Static's setup (1233 data folders, 205,556 files) the walk was 4.2 s of every
connect. Wraithguard saves the roots' lists beside itself and, next time, takes a root
from the file when none of its folders' times changed, walking only the ones that did
(`viewcore::vfsindex`, a small binary file; happy to send it). Measured on his setup,
2026-10-19: all 1233 folders taken from the saved index, the index step 0.7-2.1 s
(checking the folders' times is what is left). Tests: `viewcore/src/vfsindex.rs` (on our
side; the patch's functions are what they call).

## 0013 world: the merge's clock

Timers only, plus one `reserve`; what the merge does is unchanged. `LoadStats` gains:

- `ms_merge_loop` and `ms_geometry_wait`: your `ms_merge` is the merge loop and the mesh
  read beside it, whichever is longer; these split it into the loop alone and the time
  the loop then waited on the read (near zero on Static's setup: the loop is the bound).
- `ms_merge_history` and `ms_merge_inserts`: every 64th exterior reference timed, scaled
  up: recording its version in 0006's history, and putting it in its cell.
- `ms_merge_parts` (names in `MERGE_PARTS`): the whole loop by part, every moment of it
  in exactly one part (a small lap timer, `Laps`, one `Instant` per plugin section and per
  cell record): setup, objects, tables, definitions (lights, models, actors), interiors,
  exteriors, moves, freeing.
- `ms_defs` and `defs_counts`: inside definitions, timed in full: the lights, the models,
  the actors, and inside those the `x_twin` lookups and the twins' `.kf` reads (`timed`,
  a small wrapper); with the model records, the lookups and the `.kf` files read.

And three sizings: the history's table (`history.refs`) once for every reference the
plugins supply, before the loop, rather than rehashed at each doubling; each exterior
cell's map for its record's references; and a reference's first version with room for
one (`RefHistory::supplied`: a `Vec` takes room for four on its first push, and nearly
every reference has one version - about 250 bytes each across 8.6M).

Why: Static's 621 plugins (8.6M references) take 40-130 s in the merge, and we wanted to
know where before changing anything. Two connects on his setup, 2026-10-19 (the first
with the machine busy, so slower throughout):

| part | 1st | 2nd |
|---|---|---|
| setup | 0.2 s | 0.2 s |
| objects | 0.3 s | 0.2 s |
| tables | 6.4 s | 2.3 s |
| definitions | 19.9 s | 12.7 s |
| interiors | 9.1 s | 5.7 s |
| exteriors | 94.9 s | 17.8 s |
| (of it, history, sampled) | 29.0 s | 6.2 s |
| moves, freeing | 0.1 s | 0.0 s |
| loop | 130.8 s | 39.0 s |

Definitions are the steady surprise: one creature or animated model calls `x_twin` (a
VFS lookup) and the twin's `.kf` is read (`twin_moves_unbidden`, once per path), and the
exteriors swing with how busy the machine is, which reads like memory pressure on
millions of map entries. The `ms_defs` split is there to tell which; we will look on our side first and send what we find.
Tests: none of its own (timers); our page's boot check reads the parts back.

## 0014 world: the twin questions answered before the merge loop

`twins_ahead`, run just before the loop: every animated-class model's `x` twin, by its
mesh as written, and whether each twin's `.kf` has an `Idle` with length, each asked
once and on the pool. The loop reads the answers (a mesh it was not asked about is still
looked up there, as before). `twin_moves_unbidden` is split into `twin_kf` (the path and
the cache key) and `kf_has_idle` (the read), so both paths ask the same question the same
way. `LoadStats` gains `ms_twins_ahead` and `twins_ahead` (meshes asked, `.kf` read).

Why: 0013's timers on Static's setup put the definitions part at 12-14 s, and nearly all
of it was this: 614 `.kf` files read and parsed one after another (9.5-11.4 s), and
35,073 twin lookups (2.1 s), most of them the same mesh asked again by each plugin that
redefines an object. The answers do not depend on when they are asked, so the merge's
result is unchanged; ahead of the loop it asks the superset (every definition, winning
or not), which is the set the loop visited anyway. Your `round18cr` twins test is the one
that covers it; we could not run it here without your fixture.

## 0015 world: the mesh read's clock

Timers only. `LoadStats` gains `ms_geometry_list` (`pre_geometry`, before the merge),
`mesh_pre` (how many meshes it lists), `ms_geometry_read` (the read on the pool, wall time)
and `geometry_clock`: summed over the threads, ms reading the files, ms at the `.kf` beside
them, ms parsing, and the bytes. `read_geometry` takes a `&GeomClock` from both callers.

Why: with 0014 the merge loop on Static's setup (621 plugins) dropped to 28 s and then
waited 22.6 s for this read, which had been hidden behind it at 45-50 s. Before deciding
whether to keep the geometry between connects (as 0012 does for the file index), we want to
know whether it is the reading or the parsing.

## 0016 world: the collision read as an option

`LoadOpts::geometry`, on by default: with it on nothing changes. Off, the load skips the
read beside the merge and every read in `load_geometry`, and still counts actors, corpses
and the meshes the world places. Wraithguard turns it off because nothing in it asks
`World::geom` (it grows no grass; its collision overlay reads each mesh when shown): 0015
showed that read at 39-43 s on Static's setup, as long as the merge itself. If you would
rather have the hulls read lazily per cell for everyone, say so and we will look at that.

## 0017 world: a later record undeletes a reference

Takes round 18fc's `dead` set out of the merge (both branches, interior and exterior): the
last record for a reference stands, whole. A plugin that deletes a reference and a later
plugin whose plain record names it again bring it back, with the later plugin's placement;
a deletion that comes last still deletes it. `refs_kept_deleted` and `kept_deleted` stay in
`LoadStats` and the cell's provenance (nothing fills them now) so no caller breaks, and the
page's "kept deleted" row and connect-log fragment, which show only above 0, no longer
appear; drop them all if you take this.

Why: we checked it in the game. Four tiny plugins (Wraithguard's `tests/ingame/`, happy to
send them): a master places three gold piles in an interior; P1 edits pile 2; A deletes
piles 1 and 2; P2, after A, edits pile 1 (raised, scale 2); each of P1, A and P2 also places
a marker pile so a missing plugin shows. In vanilla Morrowind.exe (October 2026) there are
**five** piles: the big raised pile 1 is there and `ori` on it says P2's record, pile 2 is
gone, the control and the three markers stand. Under 18fc's rule there would be four. So
the game takes the later record, and the deletion does not stick.

The quote 18fc was built on ("the reference still exists and is just hidden, so moving it
just moves the hidden thing") may be about something else: a reference deleted or
disabled in a save, which the save keeps by identity. For plugins against plugins the
game's answer is the one above. OpenMW reads it the same way in its code
(`apps/openmw/mwworld/cellstore.cpp`, `CellRefList<X>::load`: a later record for the same
`RefNum` replaces the earlier one whole, `*iter = std::move(liveCellRef)`, and only a record
that is itself deleted sets `setDeletedByContentFile`); we have not run the plugins in
OpenMW yet.

## 0017, after your esp-format.md: what our test did and did not cover

Thank you for the notes (the D3 test, §7 and §9.0). Reading them against our test, the two do
not contradict each other, and we should have said what ours covered: in `tests/ingame/`
**no cell record has a `NAM0`, so every reference in it is persistent, and the cell is an
interior.** Yours are *temporary* references (after `NAM0`) in exteriors - the Seyda Neen
lighthouse (`NAM0 203`, a moved re-placement) and Wolverine Hall's flag. Where a later record
does not bring a deleted reference back may depend on which section it sits in (or on
interior against exterior, or on a plain record against a move); nobody has tried a temporary
reference in an interior with plain records.

We built that: `WG_DelT_Base.esm`, `WG_DelT_P1.esp`, `WG_DelT_A.esp`, `WG_DelT_P2.esp` are the
same four plugins and edits with a `NAM0` before the first `FRMR` of every cell record (cell
`WG Delete Test T`, `make_temporary_variant.py` writes them from the first set). Five piles
means a temporary reference is undeleted too (then the difference is exterior or move); four
means the section is the difference, and 0017 then has to carry `CellRef.temporary` (your 18gf)
into the rule. **Until that is run, 0017 is the persistent-reference rule, not a claim about
all references**; the patch's text and `kept_deleted` removal stand for that case. We will
send the result either way. If you can lend `GF_D3_Delete`/`GF_D3_Readd`, we will run them
beside ours.

Two things from your file we took on our side (not in your crates): record ids are compared
without case in our conflict scan (your §2: `Iron_Dagger` and `iron_dagger` are one record),
and interior cell names fold ASCII only (your §9.8; both references do).

## 0018 nif: the cell load's triangle reorder only when the cache is not worse

`triorder::reorder_triangles_if_not_worse(idx, positions) -> bool`: the cell load
(`nif.rs`, your 18ho call) reordered every part whether or not it came out better; a part the
file already has in good order can give back a little. The new function runs your
`reorder_triangles` on a copy and keeps it only when `analyze_vertex_cache` (cache 16) does not
get worse; bad input (indices out of range, not a multiple of three) is left alone. The call
site uses it. Overdraw is not measured here (this crate has no analyser for it; our
`meshopt-port` does, in the tool that *writes* meshes), so the rule is conservative: it also
declines the small cache-for-overdraw trade your pass allows. Tests:
`tests/wraithguard_triorder.rs` (a scattered grid is reordered and better; a good one is never
left worse; bad input untouched).

## 0019 tes3 + nif: `glam` for the terrain normals and the stand-in face normal

`glam = "0.30"`, no features, as a direct dependency of `gardenfell-tes3` and `gardenfell-nif`
(and their lock entries; your `Cargo.lock` gains the `glam 0.30.10` package, no dependencies).
Why 0.30: Greatness7's `tes3` fork, which Wraithguard builds with, asks for `^0.30`; a second
`glam` is a second, incompatible `Vec3`. Its MSRV (1.68.2) is under your workspace's 1.74. The
change is `land.rs` (`unit_vnml`, `normal_at`, `face_normal`) and `nif.rs` (`draw_from_geom`'s
flat face normal) written with `Vec3`: **the same operations in the same order, so the results
are bit for bit the old ones** - arrays stay the stored and returned types, thresholds and the
face-normal "points up" rule untouched. The old lines are kept in the tests as the reference
and every bit is compared (`land.rs` `mod glam_normals_agree`: random heights and authored
normals, blank corners, degenerate triangles; `tests/wraithguard_glam_face.rs`). Take it or
leave it: it is a readability change, not a speedup, and nothing else in your crates uses
`glam`.

## 0020 nif: a shape under OpenMW's `distortion` shader is not a surface

OpenMW's `omw:data` extra data (a `NiStringExtraData`, YAML) can ask a shape for a shader
effect. `shader: distortion:` means OpenMW draws no colour for it and bends the scene behind
it by its texture, a normal map. TR's and OAAB's animated water carries one (`Tri Water
Distortion2`, 90 frames of `omw_distortion\water\NN.dds`; OAAB's `ripples.nif` has `Distortion`
over its `Circle`); drawn as a texture it was a sheet of normal-map rainbow over the water. The
draw walk now leaves such a shape out (`omw_shader`; the refraction itself is not drawn).
`soft_effect` (soft edges, depth fade) is the other effect these files carry and is not
touched. Tests: `tests/wraithguard_omw_distortion.rs`, with `tr_water_sqstat512.nif` and
`oaab_ripples.nif` as fixtures (one TR's and one OAAB's mesh, 13 KB and 5 KB; say if you would
rather we did not put them in your tree).

## 0021 nif: a creature's own visibility and morph controllers follow the `.kf`'s clip

Greatness7, asked where a creature's animation says what is shown when: "the `.kf` has the
animation timings. Death anims play to their end and stop. Things that are supposed to be
hidden during specific bits of their animations will have their own animation timings
synchronized to what's in the `.kf`, usually via a NiVisController." That is the whole rule,
and `bind_kf` only half did it: it replaced the *bones'* windows with the idle clip but left
the file's own `NiVisController`s and `NiGeomMorpherController`s on their own windows - which
span the whole animation, every clip end to end. TR's bonelord shows it: its hood is on from
0.07 to 18.5 s and its robe from 18.4 to 20.6 s, and its `.kf` has `Idle` at 0.083-3.333 s and
`Death1: Start`/`Stop` at 18.417/20.667. The controllers run on the animation's clock; played
on a wall clock they leave the clamp (cycle type 2) and sit on the last key, hood and robe both
off - a bonelord in its bones, which is how we met it.

`follow_clip(rec, start, stop)`, called at the end of both branches of `bind_kf`: every active
embedded vis or morph controller whose own timeline *spans* the clip takes the clip's window,
looping, as the bones' tracks do; when nothing loops (`bind_kf`'s still-instant branch) they
stand at `idle_time()`. A controller that does not span the clip (a flame's blink over its own
two seconds) is not on the animation's clock and is left alone. So it is the `.kf` that
decides, for every creature, with no list of them. Tests: `tests/wraithguard_kf_clock.rs`, with
TR's `xtr_cr_bonelord_grt_01.nif` and its `.kf` (the idle clip is found; hood and robe take it;
with no `.kf` the windows are the file's own). The AutoPlay question this left open is 0022, next.

## 0022 nif: controllers with no clock of their own wait for the animation (AutoPlay)

0021 re-timed the creature's controllers that span the clip; which controllers *have* a clock
of their own is a flag, and 0022 reads it. As we read OpenMW's nif loader (`setupController`):
a controller free-runs only when the nearest `NiBSAnimationNode` (or `NiBSParticleNode`) above
it, itself included, has AutoPlay, `0x20`, in its flags; every other controller is handed the
animation's time (`AssignControllerSourcesVisitor`), which with no clip playing is 0
(`Animation::AnimationTime::getValue`). The files agree: TR's `Water` node (flags `0x6a`) and
OAAB's `Scene Root` (`0x20`) free-run, and a bonelord's root is a plain `NiNode`, so its hood and
robe are the animation's.

`free_running(rec, parent, target)` reads it. `follow_clip` now leaves the free-running
controllers alone (their own window is their clock, with or without a `.kf`), which replaces
0021's "spans the clip" guess for them, and `rest_vis` handles the case with **no `.kf` at all**:
a visibility controller that waits for an animation stands where its own function puts time 0
(`ctrl_time`: clamp, loop or reverse, as the page's `ctrlTime`), so the bonelord's hood is shown
and its robe hidden, instead of the page's clock running the controller out to its last key.
Visibility only, on purpose: a part shown or hidden cannot be nearly right; the morph
controllers were not changed for the no-`.kf` case (a moth's wingbeat may be one that free-runs
or may not, and we would rather you say than that we break it). Not done: the game runs no
controller at all on an object with no animation system (a STAT: the `still` answer of
`useAnim()`), which would need the reader to be told; the mesh reader does not know, so a
STAT's waiting controller stands at 0 too, which is where the game would also not move it.
Tests: `nif.rs` `mod autoplay_tests` (the bonelord waits; the ripples and the water free-run;
`ctrl_time`) and `with_no_kf_the_controllers_stand_at_time_zero`. We have not run OpenMW on any
of this; the reading of its loader is ours, and corrections are welcome.

## Applying them

In order, from the repository root: `for p in patches/0*.patch; do patch -p1 < $p; done`.
Checked: the series on your 18hu source gives exactly the tree Wraithguard vendors, and
your whole workspace (all ten crates, the CLI and the Tauri check) builds with it. Your
full suite, with your fixture: 1553 pass / 0 fail, against 1541 / 0 without the
patches; the 12 more are the tests we add. That check was made with 0001-0010; 0011 (2026-10-13) and 0012-0014 (2026-10-19) came after. 0011 and 0012 only add functions; 0013 adds timers, fields to `LoadStats` (built with `..Default::default()` everywhere we could find) and three `reserve`/capacity sizings; 0014 asks the twin questions ahead of the loop with the same answers. None of them should break a caller, but they have not been through your full suite yet. The same goes for 0015-0021: written against the vendored tree, which is your 18hu with 0001-0017 on it, and checked by their own tests here; not yet run through your suite.

## Your page: three fixes to the shadow receiver (`page/`)

Wraithguard's viewer page now carries your 18hu page (minus the grass tools, as agreed),
and three things our earlier copy had fixed in the sun-shadow receiver are offered back as
patches against your 18hu `page-source/` (applied in order, `patch -p1` from the repository
root; your `build.js` builds with them and the page parses; the shaders they touch compile
under glslangValidator in our build):

- **p01 - additive draws receive no shadow** (`06_mge.js`, `gdnShade`). MGE skips every
  additive draw when it lays its shadows over the frame (`rendershadow.cpp`,
  `renderShadow`: "Additive alphas do not receive shadows"). Your `uFogBlack` is 1 for
  exactly those batches, so it is one line: a flame, a glow or a spell's sheet is not
  darkened by a shadow passing over it.
- **p02 - the fade is Morrowind's near fog** (`06_mge.js`, `06_gl.js`, `37_shadow.js`).
  MGE fades its shadows by `fogMWScalar`, the game's own linear near fog (nearFogStart ..
  nearFogRange, the pair `adjustFog` fits to the weather), not by the distant land's
  exponential fog; with `airClear` the shadows run on well past where the game's stop. The
  pair is the one your `ssaoFade` already works out, handed to the shader as
  `uShadowFog` through `_shadowNow.fog`.
- **p03 - `highp` on the depth samplers that lack it** (`uShadowTex` in `06_gl.js`;
  `uSceneDepth` and the depth resolve's `uDepth` in `26_water.js`). WebGL's default
  precision for a sampler is lowp; Windows (ANGLE) ignores that, Mesa honours it, and on
  Linux (WebKitGTK) depths read through a lowp sampler band - the shadows, and the
  water's shore and refraction depth. The others in your page (the SSAO's, for one)
  already say `highp`.

## Not a patch: the rest of meshoptimizer, ported the way you ported `triorder` (`meshopt-port/`)

You ported meshoptimizer 1.3's vertex cache and overdraw passes into `triorder`, line for
line, with its analysers as the oracle, and parked quantisation and simplification in
`meshopt-plan.md`. We did the same for the rest of what a NIF tool wants, so it is done
once, and it is yours if you want it: a std-only crate, MIT (zeux's notice kept in its
`LICENSE`), every function the C++'s line for line in snake case.

- `vfetch`: `optimizeVertexFetchRemap`, `optimizeVertexFetch`, `remapIndexBuffer`,
  `remapVertexBuffer` (generic, one per-vertex array at a time).
- `analyze`: `analyzeVertexFetch`, `analyzeOverdraw`, `analyzeCoverage`.
- `simplify`: the whole `simplifier.cpp` - `simplify`, `simplifyWithAttributes`,
  `simplifyWithUpdate`, `simplifySloppy`, `simplifyPrune`, `simplifyPoints`,
  `simplifyScale`, every option and the per-vertex lock/protect/priority flags (only the
  TRACE statistics and the internal debug output left out).

Checked against the C++ itself: `tools/meshopt_oracle/run.sh` clones meshoptimizer v1.3,
builds it, runs it over the test meshes (and, given a folder, a corpus) and writes its
answers as the crate's test tables. Every value matches bit for bit, floats compared as
their bit patterns: 27 test meshes, and 4,431 mesh parts from a 768-file NIF corpus - 30
values each for the fetch order and analysers, 39 for the simplifier (four option sets,
attributes with locks, with update, sloppy, prune, points, scale). Only `sqrtf` and
`fabsf` are called, so it holds across platforms. The tests read meshes through your
`gardenfell-nif` and order them with your `triorder` (a dev-dependency). The paths in its
`Cargo.toml` and in `run.sh` are our tree's; adjust them to yours.

What we use it for: the mesh viewer's Draw order report and Optimise (your `triorder`,
then the fetch order, written back to the NIF), and later distant LODs.

## A question for after your release: a grass-free session crate?

Wraithguard takes your engine crates unmodified (with the patches above), but it cannot
do the same with `gardenfell-session`: every module in it uses `gardenfell-rules` or
`gardenfell-scatter`, and `App` holds the grass state, so taking it would bring the grass
generator in - which we agreed Wraithguard will not. So for now we carry our own copy of
the commands a viewer needs (the connect, the cell and interior data, the world map,
assets and the mesh bundle, textures, languages, a few settings) and port your changes
into it by hand; from 18hu that was `open_install`'s new load counts, the world map's sea
colour and the `still` mesh interface.

If you ever split it - a grass-free crate (the install, the world, the viewer's data
commands) with the grass part on top - Wraithguard would take the grass-free one as is,
like your other crates, and send fixes as patches against it. No hurry and no pressure:
only after your release, and only if it suits how you want the crate laid out.

## Not patches, findings you might want

- **Two file names reach your report as markup** (`11_events.js`, `openLog`): the load
  order's source (`'<b>'+GameData.orderSource+'</b>'`) and the missing plugins' names
  (`GameData.missing...join(', ')`) go into `innerHTML` through `T()` unescaped. A path or a
  plugin name can hold `<` on Linux, and a plugin's text is the user's mods, not ours: we
  wrap both in `escHtml` (and the WebGL error in `#noctx`, for good measure). Found with a
  test mode that ends every string the engine answers in markup and watches the page for it
  (`WG_TAINT=1 node boot.js`); everything else it reached was already escaped.

- **Your chain order is right and OpenMW's docs are wrong.** The "Paths" page says
  `config=` dirs are read dir1, dir2, dir3, dir4; the code (a stack, children pushed in
  reverse) reads dir1, dir2, dir4, dir3, as yours does. We had followed the docs and fixed
  ours to match yours.
- **Record cases**: `record_cases.json` (9 cases) pins the id-of-two-kinds rule both
  ways, for 0007; and two tiny plugins for checking it in OpenMW: `WG_Clash_A.esp`
  makes `wg_clash` a weapon, `WG_Clash_B.esp` (loaded after) makes it a misc item and
  places it in an interior, `WG Clash Test`. OpenMW should place the weapon.
- **Your strict NIF reader caught bad meshes of ours**: several meshes in Wraithguard's
  test fixture were malformed, and your reader (since 18gh) rightly refused them. We
  rebuilt them with your `mklamp.js`. Nothing to fix on your side.
- **Your esp-format.md on our side**: record ids compared without case (§2) and interior names folded ASCII-only (§9.8) are in Wraithguard's own conflict code now, not in your crates; the notes were right on both.
- A shared case file for the cfg reader (`openmw_cfg_cases.json`, 13 cases, every expected
  value from OpenMW's code or docs) that both our readers run; yours passes 13/13 with 0005.
