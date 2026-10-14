# Patches to Gardenfell 18hu, for Robin

From Static (Wraithguard). Each patch is a unified diff against your 18hu source, made
from the repository root (`crates/...`), and none of them touch grass. Wraithguard carries
them on its vendored copy of your seven engine crates (bytes, text, tes3, nif, bsa,
install, world) until you take them or say no; either answer is fine, we keep the
copy otherwise unmodified so your next release drops straight in. With your fixture
folder (thank you for it) your whole suite passes with the first ten applied: 1553
pass, 0 fail, against 1541 without them; the 12 more are tests we add. 0011 came a day
later (additive; see "Applying them").

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

## Applying them

In order, from the repository root: `for p in patches/0*.patch; do patch -p1 < $p; done`.
Checked: the series on your 18hu source gives exactly the tree Wraithguard vendors, and
your whole workspace (all ten crates, the CLI and the Tauri check) builds with it. Your
full suite, with your fixture: 1553 pass / 0 fail, against 1541 / 0 without the
patches; the 12 more are the tests we add. That check was made with 0001-0010; 0011 came after (2026-10-13). It only adds functions, so it cannot break a caller, but it has not been through your full suite yet.

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
- A shared case file for the cfg reader (`openmw_cfg_cases.json`, 13 cases, every expected
  value from OpenMW's code or docs) that both our readers run; yours passes 13/13 with 0005.
