# Gardenfell's engine crates, vendored

What this folder is, so nobody edits it by accident.

- **Source**: Robin Hjelte's Gardenfell (GrassForge), round 18hu, the source he shared
  on 2026-10-12 (`grassforge-main(2).zip`). MIT; `LICENSE` and `NOTICE` are his.
- **What is here**: the seven engine crates that are not grass - `gardenfell-bytes`,
  `-text`, `-tes3`, `-nif`, `-bsa`, `-install`, `-world` - byte for byte, with their
  tests, and the two data files `gardenfell-text` compiles in
  (`page-source/catalog.json`, `languages/gardenfell.en.toml`). Left out: the grass
  crates (`gardenfell-scatter`, `gardenfell-rules`) and the grass app's shell
  (`gardenfell-session`): Wraithguard is not a grass generator (our agreement with
  Robin).
- **Ours**: only `Cargo.toml` (his workspace root cut to these seven members) and this
  file.
- **Rule**: do not edit anything under `crates/`, `page-source/` or `languages/`. A
  fix goes to Robin; until he takes it, it is carried as a patch in `patches/` and
  listed below, so the next update is a copy plus those patches.
- **Updating**: replace the seven crates and the two data files from his new source,
  re-apply `patches/`, regenerate the hashes below
  (`find . -type f ! -name VENDOR.md ! -name Cargo.toml ! -name Cargo.lock ! -path './target/*' ! -path './fixture/*' ! -path './patches/*' -print0 | sort -z | xargs -0 sha256sum`).

## Tests

`cargo test` here (2026-10-12, Rust 1.91, our patches in): 503 pass, 0 fail - his 491
and 12 of ours - with his generated test fixture in `fixture/`. Without it 32 of his fail
(they read `morph.nif`, `skinned.nif`, `detail.nif`, `packed.bsa`, a fixture install).
Robin sent the folder (2026-10-12); it is not in the repository (it holds two copies of
a real Russian `Morrowind.ini` under `fixture/ru/`), so CI runs only our own tests in
these crates.

## Patches carried

Applied to the files under `crates/` (the hashes below are upstream's originals, so these
files differ from their hash on purpose). With neither hook set, his engine reads exactly
as before (his tests: all pass, as unpatched).

- `patches/0001-world-record-typer-hook.patch`: `gardenfell_world::world::set_record_typer`,
  the one call that types a record (`Object::from_raw`) made through a hook that defaults
  to it.
- `patches/0002-nif-stream-reader-hook.patch`: `gardenfell_nif::nif::set_stream_reader`,
  the one call that parses a mesh (`Stream::parse`) made the same way.

Wraithguard sets both at start-up to the adapter (`viewer-shell/gardenfell-g7`), so
Greatness7's `tes3` crate reads, and Robin's code runs on what it read.

- `patches/0003-vfs-providers.patch` (redone 2026-10-12 after Robin's review: one walk
  with the resolver, so the list's first is the file loaded; `texture_providers` and
  `mesh_providers` with and without the prefix; `tests/wraithguard_providers.rs`):
  `Vfs::providers` (which folders and archives
  supply a path), needed by the viewer and impossible outside the crate (`Root::index`
  is private).
- `patches/0004-weather-ten-weathers.patch`: the ten weathers as an opt-in
  (`ALL_WEATHERS` and `ini_json_with`, `mge_json_with`, `openmw_to_ini_with`,
  `atmosphere_json_with`; the plain readers keep Robin's Clear only, round 17h), and
  `glareView` in each weather's JSON. Sky, fog and water only, no particles. Tests:
  `crates/gardenfell-install/tests/wraithguard_weather.rs`.
- `patches/0005-openmwcfg-quoted-path-tail.patch`: a quoted path with text after the
  closing quote keeps the quoted part, as OpenMW does.

- `patches/0006-reference-history.patch`: `World::history` (`History`, `RefHistory`,
  `Pose`): every plugin that supplied a version of each reference and its placement in
  each, filled by the merge (the inspector's version history, the review marks). Kept
  beside the cells, not in `CellRef`, so nothing that builds one changes.
- `patches/0007-openmw-kinds-option.patch`: `LoadOpts::openmw_kinds` (off by default,
  Robin's 18gn rule unchanged): OpenMW's rule for an id defined as two kinds, with tests
  (`crates/gardenfell-world/tests/wraithguard.rs`). Wraithguard turns it on.
- `patches/0008-nif-gloss-embedded-vis-bone-names-skeleton.patch`: the mesh reader's
  additions the viewer draws with - the gloss map, embedded textures' names,
  NiVisController visibility, bone names, skinned parts kept whole, and
  `nif/skeleton.rs` (a skeleton read once; body parts bound to it by bone name, as
  OpenMW binds them; `bind_kf` uses `KfData::window` to carry only one clip's keys,
  including boundary keys for interpolation). The binding call lost during the 4.4
  rebase is restored in this patch; the synthetic skeleton binding regression test
  runs with `cargo test -p gardenfell-nif --lib`, without the external fixture.
  Skeleton binding clears node-motion links while preserving the part's alpha and
  visibility tracks; regression coverage checks both moving and baked skins.
- `patches/0009-cell-history.patch`: `History::interiors` / `exteriors`
  (`CellHistory`): every plugin with a CELL record for a cell, and the references a
  later plugin deleted or moved away (`Gone`). Also beside the cells, not in `Cell`.

- `patches/0010-bsa-open-not-read.patch`: an archive is opened, not read - `Bsa::open`
  reads the header and tables and keeps the file open, `Bsa::read` reads an entry on
  demand (a positioned read), `Vfs::load` uses it; `get` still lends a slice. Reading
  every archive whole (his design) put gigabytes in memory on a modded install and froze
  the viewer's cell load (R4, 2026-10-12): 800 MB archive, 21 s and 804 MB to open
  before, under 1 ms and 4 MB after. Tests: `crates/gardenfell-bsa/tests/wraithguard_bsa.rs`.

- `patches/0011-ranged-read.patch`: part of a file without the rest - `Bsa::size` and
  `Bsa::read_range` (a positioned read inside one entry), `Vfs::size` and
  `Vfs::load_range` (a loose file seeked, an archive entry read at its offset). The
  viewer reads a DDS's header, then only the mip levels its Texture detail keeps
  (`viewcore::img::dds_trim`): a cell on Static's setup read 1.2 GB of textures to send
  15 MB. Additive. Tests: `crates/gardenfell-bsa/tests/wraithguard_bsa.rs`.

- `patches/0012-vfs-saved-index.patch`: a root's index kept between connects -
  `Root::index_one` (one root walked as `Vfs::with_roots` walks it, and every folder the
  walk read), `Root::from_index`, `Root::index` and `Vfs::from_roots`. The walk is
  unchanged; it only also says which folders it read, so the viewer can tell a saved
  index from a stale one by those folders' times (`viewcore::vfsindex`). Additive.
  Tests: `viewcore/src/vfsindex.rs`.

- `patches/0013-merge-timing.patch`: the merge's clock, for the connect log - `LoadStats`
  gains `ms_merge_loop` and `ms_geometry_wait` (the loop, and its wait on the mesh read
  beside it), `ms_merge_history` and `ms_merge_inserts` (every 64th exterior reference
  timed, scaled), `ms_merge_parts` (the whole loop by part, `MERGE_PARTS`, nothing
  sampled: they add up to the loop), and `ms_defs` and `defs_counts` (inside definitions:
  the lights, models and actors, and the twin lookups and `.kf` reads within them). And
  three sizings: the history's table once for every reference before the loop, each
  exterior cell's map for its record's references, and a reference's first version with
  room for one (`RefHistory::supplied`). The first timers and the sizings went in with
  4.5.0's merge work (2026-10-18) without a patch, which is why 0006 stopped reversing
  cleanly: the sizing sits inside 0006's lines. Made against the file as of commit
  1a21c43 (this list's hash); the series 0001-0014 now reverses to it exactly (checked
  2026-10-19). What the merge does is unchanged. Tests: the boot check
  (`ui/tests/boot.js`, the parts named).

- `patches/0014-twins-ahead.patch`: the merge's twin questions answered before its loop,
  on the pool (`twins_ahead`): every animated-class model's `x` twin, by its mesh, and
  whether each twin's `.kf` has an `Idle` with length, both asked once per mesh and in
  parallel; the loop reads the answers (a twin not asked ahead is still looked up there).
  `twin_moves_unbidden` is split into `twin_kf` and `kf_has_idle` so both use the same
  key and read. `LoadStats` gains `ms_twins_ahead` and `twins_ahead`. On Static's setup
  the loop asked 35,073 twins (2.1 s) and read 614 `.kf` files one by one (9.5-11.4 s).
  The answers do not depend on the order they are asked in, so the merge's result is the
  same. Tests: Robin's `round18cr` twins test needs his fixture (not here); the boot check
  reads the counts.

- `patches/0015-geometry-clock.patch`: the mesh geometry read beside the merge (round
  18cr's), timed: `pre_geometry`'s listing, how many meshes it lists, the read's wall time
  on the pool, and `GeomClock` (summed over the threads: reading the files, the `.kf` beside
  them, parsing, and the bytes), passed to `read_geometry` from both its callers. Timers
  only. On Static's setup (2026-10-19), with 0014 the merge loop finished first and waited
  22.6 s on this read: it is the merge's bound now.

- `patches/0016-geometry-optional.patch`: `LoadOpts::geometry` (on by default, so
  Gardenfell reads as before). Off, the load skips `pre_geometry` and every mesh read in
  `load_geometry`, keeping the counts that need no mesh (actors, corpses, `mesh_wanted`).
  Wraithguard turns it off (`viewcore::world::World::load_with`): nothing in it reads
  `World::geom`, which is for the grass it does not grow, and the read was 39-43 s of
  every connect on Static's setup (33,267 meshes, 1.78 GB; patch 0015's numbers).

- `patches/0017-later-record-undeletes.patch`: round 18fc's `dead` set taken out of the
  merge (interior and exterior branches): the last record for a reference stands, whole, so
  a later plugin's plain record brings back a reference an earlier plugin deleted, and a
  deletion that comes last still deletes it. `refs_kept_deleted`/`kept_deleted` stay (now
  always 0). Checked in vanilla Morrowind.exe with `tests/ingame/`'s `WG_Del_*` plugins
  (five piles: the later record wins); the same rule as `ordercore::refs`, pinned for the
  engine by `move_cases.json` ("deleted, then moved").

- `patches/0008-followup-preserve-part-tracks.patch` is already inside 0008 (and sorts
  before it, so a `for p in patches/0*.patch` would stop on it): delete it.

0001-0011 are offered to Robin (0012-0017 not yet; the 2026-10-19 zip has 0001-0014), with `patches/NOTES-FOR-ROBIN.md` saying why each.
Applied in order to his 18hu source they give exactly the files here (checked); his
tests: 503 pass with his fixture (his 491 and 12 of ours). Only 0006 and 0009 add
a field to an existing struct, and only to `World`, which every caller builds with
`..Default::default()`.

**Check every patch against his whole workspace, not only these seven crates**: his
grass crates (`gardenfell-scatter`, `-rules`) build `Cell` and `CellRef` field by field
in 62 places, so a new field there compiles here and breaks his tree (the first 0006 and
0009 did), and with his fixture in place, not without it: the first 0004 changed what
his Clear-only weather tests pin, and only the fixture runs them. Done 2026-10-12 with
the series and his fixture on his full 18hu source: it builds, and his whole suite gives
1553 pass / 0 fail against 1541 / 0 unpatched (the 12 more are ours).

- `patches/0018-cell-load-reorder-if-not-worse.patch`: the cell load's triangle reorder
  (`nif.rs`, Round 18ho) kept only when the part's vertex cache does not come out worse
  (`triorder::reorder_triangles_if_not_worse`, so a mesh already in good order is drawn as
  the file has it). Tests: `crates/gardenfell-nif/tests/wraithguard_triorder.rs`.

- `patches/0019-glam-for-terrain-and-face-normals.patch`: `glam` 0.30 (no features: the version the
  `tes3` fork already pulls in, so there is one `Vec3`) as a direct dependency of `gardenfell-tes3`
  and `gardenfell-nif` (and their entries in `Cargo.lock`), and the vector arithmetic of
  `land.rs` (`unit_vnml`, `normal_at`, `face_normal`) and `nif.rs` (`draw_from_geom`'s flat face
  normal) written with `glam::Vec3`. The same operations in the same order as before, so the
  results are **bit for bit** the old ones; the stored and returned types stay arrays. Tests: the
  old hand-written lines kept as the reference in `land.rs` (`mod glam_normals_agree`) and
  `crates/gardenfell-nif/tests/wraithguard_glam_face.rs`. Not offered to Robin yet (the workspace's
  rust-version 1.74 is above glam 0.30's 1.68.2; his other crates are untouched).

- `patches/0020-omw-distortion-shapes-not-drawn.patch`: a shape whose `omw:data` extra data asks
  for OpenMW's `distortion` shader (TR's and OAAB's animated water has `Tri Water Distortion2`
  over the water, 90 frames of `omw_distortion\water\NN.dds`) is left out of the draw walk
  (`nif.rs`, `omw_shader`): OpenMW draws no colour for it, it bends the scene behind by the
  texture, a normal map; painted as a surface it was a sheet of rainbow. Test:
  `crates/gardenfell-nif/tests/wraithguard_omw_distortion.rs` with the mesh in
  `tests/fixtures/tr_water_sqstat512.nif`.

## File hashes (SHA-256)

```
181b42649668f301e29a3319e3ba8dea7bc28ade55912522de618545465843a2  ./LICENSE
e995c8f226d8481fac8e0d61b56821a12ef6ebe216d2f26c01b34e15a911b0b9  ./NOTICE
386435ec87dc2eaa6ced73b82d825944e330578d98f668a7f5d90e865a029e70  ./crates/gardenfell-bsa/LICENSE
99279a21416a21048ca1eeb2fab8ad3fb6b103b3a8ed19765afc964279f67947  ./crates/gardenfell-bsa/src/bsa.rs
fc0b58216ff5adb5e59722e727a6ebffbe307300bbf0fe1db05a0f6e66e4d340  ./crates/gardenfell-bsa/src/lib.rs
9749b1c9d7d7fa4719ecbd9c8c1e1eec64a09af1348915f2a507e3e01e563858  ./crates/gardenfell-bsa/tests/alpha.rs
f3d0e73c74da579bcee9756fa0b424b5f6428d6d5011432ad2ceff9ac743bab2  ./crates/gardenfell-bsa/tests/wraithguard_bsa.rs
386435ec87dc2eaa6ced73b82d825944e330578d98f668a7f5d90e865a029e70  ./crates/gardenfell-bytes/LICENSE
c33d088a4f058300d05d8c38e1086acaec34c08d47bd7533f6e996e7ce3efcca  ./crates/gardenfell-bytes/src/b64.rs
cd04910c4f3d553be7b52fc05300d93bc776297ea0a2403fa7083f45cfc7e019  ./crates/gardenfell-bytes/src/bytes.rs
97969a7cf235afdd4d1c1d7f0b2554bea958039626402d0e339fabc52cc53be6  ./crates/gardenfell-bytes/src/codepage.rs
6c49f041d63009e24fe07cc286a985d876a842dd24a483163dd342b452a3a1c4  ./crates/gardenfell-bytes/src/geom.rs
776c71b9ce589c75e821a72aed57fc0e2ca9fab759b0f96861b1682c616285d0  ./crates/gardenfell-bytes/src/img.rs
eed942339a262e5e8e8f3eda3d705c7684c9804707cf3047f4c780ad28ade8d5  ./crates/gardenfell-bytes/src/ini.rs
55594ed39886879d985d27c979e2873e90ced6902c9366f65b4211ee6ddb4133  ./crates/gardenfell-bytes/src/json.rs
247d1b2ff46a645dc19076e7ae235a3e841a032f0e4df18fa992179705bce063  ./crates/gardenfell-bytes/src/lib.rs
8987d55431ee729dadc89324aede67c22ed8b0a1fd7c26a1cf81a8acc032e4a6  ./crates/gardenfell-bytes/src/openmwcfg.rs
ecee60a3d350288db8912afc2bf0b0f72623c04c75c34b2b18c7b5ade0c1bbcb  ./crates/gardenfell-bytes/src/paths.rs
1ec52c5016c280f9abf31da94c609a63970f23fe062ba4b9c1a6b1947cf86e49  ./crates/gardenfell-bytes/src/pool.rs
5744cf1c901450fdde6c0416a38c85388bf73569ad5edd15af0199591dc45320  ./crates/gardenfell-bytes/src/segdist.rs
c3fb55f4160992c7ba8ec5db89254725ac28accf2914ae86f735203be3b5d2a1  ./crates/gardenfell-bytes/src/toml.rs
8d97cd2c9fad95e8d9c3b0bd706ccb0e6396facb14827ea8a1c2baa4897d3166  ./crates/gardenfell-bytes/src/triaccel.rs
d39192e12e1fd61c62c2637b670c19fb1fd49feeb923428637e9da2390068cc6  ./crates/gardenfell-bytes/tests/img.rs
1562f23fdd89b4d358a102cdbcec566a48cdc322acbf1aedfff14ec6752e5784  ./crates/gardenfell-bytes/tests/json.rs
386435ec87dc2eaa6ced73b82d825944e330578d98f668a7f5d90e865a029e70  ./crates/gardenfell-install/LICENSE
93487c869956783d5467f5094abc45b5c6b177fadbf84f62348aecf389a2b48f  ./crates/gardenfell-install/examples/bsaget.rs
628185842fca9bcfbf7fe7aaf3ed9098d39d6ae8202f0f032bf1054730132a38  ./crates/gardenfell-install/src/layout.rs
7e3df5dc6fb20c8fda032fd37955fd1a49d853bf7168399916dda2f3ee46271f  ./crates/gardenfell-install/src/lib.rs
44090c2228c2b37006b9a84569e90ce2efb4309b17ad94eacc9ea80e7dedb944  ./crates/gardenfell-install/src/mge.rs
b6f34ad9a75666467c6a5450ae8c416ec99eceb8bfd68f6012c51e8e274f6c48  ./crates/gardenfell-install/src/plugins.rs
5a0c042bdfd4bf170db1424378e5822859e5697f2ac5bbb1fe705aa9aa61eaaf  ./crates/gardenfell-install/src/renderer.rs
c5030f338d1a95b67a239466bf9c27aa94c3aafbbca35288f8c06cd7831e298a  ./crates/gardenfell-install/src/vfs.rs
98ed3a80d6edeec714e56938683a334bfe8ae98544e13cff82565661dfaba7e6  ./crates/gardenfell-install/src/weather.rs
ed605b6e092197c3c157b9c688fc51dbe02ca51bed132c27044fbb7a921cef3d  ./crates/gardenfell-install/tests/layout.rs
d2f5c3a7cd38a47edcdf4998e4f2cdd52d210b386dde300ef99f1473a53b5915  ./crates/gardenfell-install/tests/mge.rs
80d2a172271b53d421a206f7b99bd05d6ed5b970e3858dc8ce0b3506c48c7592  ./crates/gardenfell-install/tests/overlay.rs
2f0b02996a4a5c66487ddec42e38ee3a8daeed71d6fbb2d764e2794874781f59  ./crates/gardenfell-install/tests/renderer.rs
1bd48049d5dd89ad10674c0347a005ce200b7e6134b1a489098a47873f44f21d  ./crates/gardenfell-install/tests/round18bi.rs
aedf81e0eb767735d0ea9ec6a2085a50127dfeaa95d7d9ed5d5e84bdff430ab2  ./crates/gardenfell-install/tests/round18by.rs
993b5a689007435a701c6a98ed987d0e9d8b1c379f91d4396ecbfd188c32f48d  ./crates/gardenfell-install/tests/round18gn.rs
78b7dad1d27fb4c2546b92c1218966616957b65dea8b565ebf80cb22e0c76ba1  ./crates/gardenfell-install/tests/round18gq.rs
ed297aedc6821d31dc0144f939c6a8879af55f1a3d3801687f58b3dc797f260d  ./crates/gardenfell-install/tests/weather.rs
e7711aaf48d1dca500a122504e363c35e2bfedc9e2d0d5d15861986ff4cf7751  ./crates/gardenfell-install/tests/wraithguard_providers.rs
3905e16d7f97f1dc0a2307462fd728f1fc5b9bd4eab93417c1ac1b8814ec3ab2  ./crates/gardenfell-install/tests/wraithguard_weather.rs
386435ec87dc2eaa6ced73b82d825944e330578d98f668a7f5d90e865a029e70  ./crates/gardenfell-nif/LICENSE
88c430401f5ecefb5271c5c286ec1686ae13324bb2255b4616a9b26884978c5c  ./crates/gardenfell-nif/examples/bladeh.rs
e0716478173e3982598407e30794d53ba9f92e8e153c1cb2cbea4f145b22dd40  ./crates/gardenfell-nif/examples/flipdump.rs
3043d91e17b8761b67a6b5168965972a5c440f14a065d0257bc8f686897f6a54  ./crates/gardenfell-nif/examples/kfdump.rs
49ee459f8662c1db9b6ccd0041e30b7046a4b7c061fb10fd5fb21808320e770f  ./crates/gardenfell-nif/examples/kfkeys.rs
0fd34bce13647d751cbbf8ba4b0cc51cd3ea29276e2fc706872acdb00cbef2c9  ./crates/gardenfell-nif/examples/kfmotion.rs
1972fd4f500c478fb1c2033fc6d7ceef9a9cf0c886c5374b10dab08db20b8721  ./crates/gardenfell-nif/examples/lamplit.rs
fe4e02aedc3da10e9ddc0ff509b9a6d9140687cb8f04c1786919ab70d64f3d20  ./crates/gardenfell-nif/examples/meshdump.rs
1144eebd5f882aa455dc25e41afa5301932d85f46681edadf43b3567d0c357d9  ./crates/gardenfell-nif/examples/nodetree.rs
df3d99199e54c912ea8d8caa1fc75b773b4657144a1cc5ab3fc3b353a7c94f0d  ./crates/gardenfell-nif/examples/partdump.rs
37138e65e6a25f3d7c87d0145b3d913e18969298688b4506b751bf12a6282b9b  ./crates/gardenfell-nif/examples/particles.rs
2114c06f9b7de9abcd15b4b432379746475b8059aebfd876334de6b56a50fe54  ./crates/gardenfell-nif/examples/partstats.rs
9ea419cae64f17920b0351bedd8fad4981de144cdbc4d6dfe797059e4c271fd3  ./crates/gardenfell-nif/src/lib.rs
63e76a149f847f232ea7676eab9485fcc9ca48de0a0676b71d0c3fa2b913c9e1  ./crates/gardenfell-nif/src/meshscan.rs
c657bf8dab46486df8c840c32d58ab2f2ece0fc5bfaf21f2c88562e3e8fb32b6  ./crates/gardenfell-nif/src/netimmerse/check.rs
548ac5f0f708d61ca582ce6019d92f85ac603b267314bd32e91fd7552ebcaaef  ./crates/gardenfell-nif/src/netimmerse/mod.rs
71df4465c9527dc32b8a3fd0647368e3b84c717230bf6c4a0f70b5055062235b  ./crates/gardenfell-nif/src/netimmerse/stream.rs
d4f5768b1f63f7b7f1077491f5184cbb4fc85f069a3ac3c14b6078ed53eea90c  ./crates/gardenfell-nif/src/netimmerse/types/avoidnode.rs
71474df93de31ecd91baa1892f6e156cc28e3c7ab0d0a94eae6c7f3bc3807c6f  ./crates/gardenfell-nif/src/netimmerse/types/brickniextradata.rs
21f4716c8bbcc2ecd4775e9eb48c1231cd0de596046ebfb9bfbab34243d8a147  ./crates/gardenfell-nif/src/netimmerse/types/bsmirrorednode.rs
73b24c9e6721aecbd66e1ea5b31a1064b67400227d33a1c22f9143ba42bbf1f1  ./crates/gardenfell-nif/src/netimmerse/types/consts.rs
67805a60629637457a150185457053614cffcc6178ded423e04f7fad083dd8d6  ./crates/gardenfell-nif/src/netimmerse/types/mod.rs
73efa81ff80e206c81cce484e1d7a051d64d45680712a94d3d25fcbb6e6f749a  ./crates/gardenfell-nif/src/netimmerse/types/niaccumulator.rs
1644614af57ee23c25a578169b55e5b6fddb71b53eafa5cf87e4702c19c48f55  ./crates/gardenfell-nif/src/netimmerse/types/nialphaaccumulator.rs
7cb60b90174587d0013c513972446fa1a7435085712e5e1efb404ad02cf231c4  ./crates/gardenfell-nif/src/netimmerse/types/nialphacontroller.rs
f9c01a46c80421f6ebf496060a1e703fb156dcac1534fc49648cc0a65702f979  ./crates/gardenfell-nif/src/netimmerse/types/nialphaproperty.rs
92abc243db4abe1e7f64f7db32cb3444c2c6ddd187eee07336f69bccd4370f67  ./crates/gardenfell-nif/src/netimmerse/types/niambientlight.rs
390263c60374abc551624d6f03992449a6364e38d5f32bd67f98d0a84b704f2d  ./crates/gardenfell-nif/src/netimmerse/types/niautonormalparticles.rs
e10a185042814610156eea0c3af23595e0ad0b5202c12e879e62a42cac1ad396  ./crates/gardenfell-nif/src/netimmerse/types/niautonormalparticlesdata.rs
b36265e296cd578bc1bd0ebe8a4334d7860599c5b2643e9d3a08af0ca6156c07  ./crates/gardenfell-nif/src/netimmerse/types/niavobject.rs
c28dc81c31a32ef5edd2965a6e6252078cd90fe97dca48c4882986977e846380  ./crates/gardenfell-nif/src/netimmerse/types/nibillboardnode.rs
83754139577d1e89a249e50e61b2757ec6356949ea2c1c86f16e509b962e0028  ./crates/gardenfell-nif/src/netimmerse/types/nibltsource.rs
6c1c4569ddc02dbdcae9a924813583dddc2906858cfc2678531f61e2a0cf6c88  ./crates/gardenfell-nif/src/netimmerse/types/nibound.rs
6e44db9315dca8dd6da99f57cf20dda91f74d4f8d1e906eae41ba478d4cc7eac  ./crates/gardenfell-nif/src/netimmerse/types/niboundingvolume.rs
b4c60dc6c5504420df56cbf05f96ff3435b12f7449c7c53779eead64b780a915  ./crates/gardenfell-nif/src/netimmerse/types/niboxbv.rs
932a9f2344397a423b9753750b6122a6e7aedca0b45aa9374b60f909848fb54a  ./crates/gardenfell-nif/src/netimmerse/types/nibsanimationmanager.rs
7721ff7859c33b378e48843d602f2ff08fd3736e94bd121fbb8328f328a17f1f  ./crates/gardenfell-nif/src/netimmerse/types/nibsanimationnode.rs
047f70840cca025debd849291848b4947fbff255114217d76543b951b6fdd448  ./crates/gardenfell-nif/src/netimmerse/types/nibsparraycontroller.rs
d89fc9d4b6b140ff1fe9d980123a07f45af7c584d33dadf9a4f951b18e80c22a  ./crates/gardenfell-nif/src/netimmerse/types/nibsparticlenode.rs
93f25a9087517820b01d667f7edb4002f90ec871e8856ec60f9b84618042e3b1  ./crates/gardenfell-nif/src/netimmerse/types/nibspnode.rs
9813b7cd611a35d20731a63faac15da6fc4e82986985970145220c3737e9cd59  ./crates/gardenfell-nif/src/netimmerse/types/nicamera.rs
8e8450a566cea00de9d8b255a140af15fd563a60944c6a3b6099eb4792f412ef  ./crates/gardenfell-nif/src/netimmerse/types/niclusteraccumulator.rs
af333e32bf82789dc73ef33722e03c90bc3b63bab57be861266613fae6762e81  ./crates/gardenfell-nif/src/netimmerse/types/nicollisionswitch.rs
d804f393725be4798df13e1cccedc3e00b50a1fc5b7ccf3ad497e3108af22b9f  ./crates/gardenfell-nif/src/netimmerse/types/nicolordata.rs
2ccff9a2b88d706fd5b8e723c5a02cf57f09d5cd5e384840b8dda5fe864e4b7c  ./crates/gardenfell-nif/src/netimmerse/types/nicolorkey.rs
a96b93995b4b854329781e48980750762db20b30a6be913f97408970b1235283  ./crates/gardenfell-nif/src/netimmerse/types/nidirectionallight.rs
447d494e9275f87af371f2ae6581ef49fcf71a86988f347b61137ce0c3a834fe  ./crates/gardenfell-nif/src/netimmerse/types/niditherproperty.rs
42b8351ccc9fa62b3a1f2df78831ce6f2db675665f3147be8f09c1fb2414cc8c  ./crates/gardenfell-nif/src/netimmerse/types/nidx8renderer.rs
310bd6d4c2e99b127b7f72bfafa2feae9fe38244764a9008a0f8eb4c66012295  ./crates/gardenfell-nif/src/netimmerse/types/nidynamiceffect.rs
cb43a549e2609a6ca8ebaf1d3d1b1b6719bae7d6d7773501fec54f5e3fa3511f  ./crates/gardenfell-nif/src/netimmerse/types/niemittermodifier.rs
f1366afe1ab28cb36f160a07611b354848035216f9972d9b3a6bfd759973e271  ./crates/gardenfell-nif/src/netimmerse/types/niextradata.rs
3f994a54d9b23a7a2239b1216e1e99605983864f7b781185d7f5b7c74cf4b6a2  ./crates/gardenfell-nif/src/netimmerse/types/niflipcontroller.rs
9c07ae702e01c944ce92b8945d38f7722e91e12bbc4d2adb5f5987ba21e8079f  ./crates/gardenfell-nif/src/netimmerse/types/nifloatcontroller.rs
ad2e9dbddb202a7989250943f4f3027a7c331355f400512eaccd22f3053db63f  ./crates/gardenfell-nif/src/netimmerse/types/nifloatdata.rs
f2ae5526cd4842f23bffe26ccbe032204b1eb653ab3b6aa32851a13563434819  ./crates/gardenfell-nif/src/netimmerse/types/nifloatkey.rs
dd68de7b95f7ce14be29a156fa57cf56861f4ba1c4560ae44cfc1a024cf8ee9d  ./crates/gardenfell-nif/src/netimmerse/types/nifltanimationnode.rs
373f30af2e379980fca66be33912e9f81db41bfee0b1b4963a7975c35ce5b23c  ./crates/gardenfell-nif/src/netimmerse/types/nifogproperty.rs
a2ea832e5715b0cef51f60866b164be74439f91c09d60c7bace64f69bfb5b572  ./crates/gardenfell-nif/src/netimmerse/types/nigeometry.rs
e9d07a08f75f6f4fa34c1fedf29430f3e7cadc3de26540bc43163cca64b5434a  ./crates/gardenfell-nif/src/netimmerse/types/nigeometrydata.rs
6a801e4eeaab7cd4abd563a8a2cfead953b9eb3828c3551f3f76ab308f9bbb38  ./crates/gardenfell-nif/src/netimmerse/types/nigeommorphercontroller.rs
1d2afcb9402017d0472618224888eaf706ec7bd0761b6b8d7d7f1b0763b7a93f  ./crates/gardenfell-nif/src/netimmerse/types/nigravity.rs
828d881924d8783261d0156f6ca3f6ddd3d1b3863a4989ea0697eee351adc1a3  ./crates/gardenfell-nif/src/netimmerse/types/nikeyframecontroller.rs
34ca1ec4224cdf40f573548459633c7b5b0cfb9e67b004c454550810bede73e3  ./crates/gardenfell-nif/src/netimmerse/types/nikeyframedata.rs
bc3e7a55e659a390eda79ad9d893e33c9eaf514ae501f4b84d59f867e4449c2d  ./crates/gardenfell-nif/src/netimmerse/types/nikeyframemanager.rs
69febbe9d982b340f196d9a1457c61ece49d277032295650ed1e1eaf68299c2a  ./crates/gardenfell-nif/src/netimmerse/types/nilight.rs
c38081176f5b28253fb3348632d7af846dbe22b95e61e8ae8a8ad01b910c749b  ./crates/gardenfell-nif/src/netimmerse/types/nilightcolorcontroller.rs
7be9838bd8a51918dec3e3ea13aed147dd0d84ba13effd058ef07c23c0438032  ./crates/gardenfell-nif/src/netimmerse/types/nilines.rs
f4bb293974dcbd7e7fbad4a34bb4ddb1301ef709159db6fb6ce68721ef65e439  ./crates/gardenfell-nif/src/netimmerse/types/nilinesdata.rs
20d58003ad3c9a9043093b9c193a37e2bb3958fb9555f0011ef78b2c5073e2c7  ./crates/gardenfell-nif/src/netimmerse/types/nilodnode.rs
07f6cfdb31633e71e64eb91e37a384fe68b6b23a926e0ff145127e0c422a3bac  ./crates/gardenfell-nif/src/netimmerse/types/nilookatcontroller.rs
84b77f7ebaf4ae006b15b98d52b443f0e58d4acd9959729efc3d040929056fa5  ./crates/gardenfell-nif/src/netimmerse/types/nimaterialcolorcontroller.rs
e1298cfc2831ca849831d432bad69d2fb410be11aeb8807f7c935001fba8f812  ./crates/gardenfell-nif/src/netimmerse/types/nimaterialproperty.rs
8b311748db3a41af880103a56e94206a5a1bbe04f2ec32bb0072a10f86f95c84  ./crates/gardenfell-nif/src/netimmerse/types/nimorphdata.rs
a6e02455d771aa62b4fb7a8bad04f858508ba232ecb8e69f10a70d0363eb0fab  ./crates/gardenfell-nif/src/netimmerse/types/nimorphercontroller.rs
3c76b459f1890dfc8739e6358a67298d22a4882dd9d858921dc65647aa4b634f  ./crates/gardenfell-nif/src/netimmerse/types/ninode.rs
1b3a1e15ad5561643c324c37c266d8fd760835dcf001650af91ffdf5079b5006  ./crates/gardenfell-nif/src/netimmerse/types/niobject.rs
5065a60de1607fcbd351bbc146ff00c2eb45afd28c2342c13c6bbb83ba8cb833  ./crates/gardenfell-nif/src/netimmerse/types/niobjectnet.rs
a582c1291d95343b1695dc1f260c6479b4cc585624d9f945024ed2c138fd09ea  ./crates/gardenfell-nif/src/netimmerse/types/nipalette.rs
d2de6a3e7aa8918f99cd3a6d2a5b79299eaf5eb6161b79cfead48a66e91336e9  ./crates/gardenfell-nif/src/netimmerse/types/niparticlebomb.rs
42c8b1f46578978284415f5a90a6ffdaadc3b822388f8c82d7d305b4e98af7ad  ./crates/gardenfell-nif/src/netimmerse/types/niparticlecollider.rs
dbd3a9082f04a6943be0d99b9a0341471871129855c6e156226cf5d9ed18fc84  ./crates/gardenfell-nif/src/netimmerse/types/niparticlecolormodifier.rs
622240df35f8392df950c632acb335ebe05af9cb125369d34a04eb4a639043ef  ./crates/gardenfell-nif/src/netimmerse/types/niparticlegrowfade.rs
9450b05b83f3fc5a4ee1cbad3bc61e015d71b0b99b20047b0188643cd8028f07  ./crates/gardenfell-nif/src/netimmerse/types/niparticlemodifier.rs
479b0b5ff3bb6fe085dd3d4abe0de8adad02cde5d2c47a23669dc7b6bbdbb31c  ./crates/gardenfell-nif/src/netimmerse/types/niparticlerotation.rs
04198b8bdfbf1e5cb9e101d0d88880b83d6bb74e5024cbd47bdb035ce32663c1  ./crates/gardenfell-nif/src/netimmerse/types/niparticles.rs
76b9082946faf534c82e2ebee2398babd160bb666da57b17ce5546ee0a6fd69d  ./crates/gardenfell-nif/src/netimmerse/types/niparticlesdata.rs
c8d3dd8f960fa0e1554a4763665135e61d0a623da3d821b769fa973d87d16a46  ./crates/gardenfell-nif/src/netimmerse/types/niparticlesystemcontroller.rs
94690a8a2d9b513a7fbdbe0e7be36634ad425ad75be70328dd58051172405317  ./crates/gardenfell-nif/src/netimmerse/types/nipathcontroller.rs
e09b05b9ad3f1a909b6a43dce4d7702bf02193499b68bb85144a933e80318977  ./crates/gardenfell-nif/src/netimmerse/types/niperparticledata.rs
441f99194f3fd4be39bba11f2f141daf70ce94a5fcd6a297643060f6ce2555d8  ./crates/gardenfell-nif/src/netimmerse/types/nipixeldata.rs
af70873bd9d65e919b3f410abd311f228495651fb696b0b2187ffde6ccc4ae9d  ./crates/gardenfell-nif/src/netimmerse/types/nipixelformat.rs
d7473e575d209c246ee7ac9c145a1469e3966ee70a3fe32dbb14588e3f7b10b0  ./crates/gardenfell-nif/src/netimmerse/types/niplanarcollider.rs
136e98fde1443d55cb9044eeeac230ee9e7e9d4aca82533ac97d4f97de845cb5  ./crates/gardenfell-nif/src/netimmerse/types/nipointlight.rs
c81418b2857e6d973868fcf995a0ec3aec7f3fb0e575901a443bb330a615d300  ./crates/gardenfell-nif/src/netimmerse/types/niposdata.rs
5d7f585ed2f7c8087892ce09aaea35caa6c1af16b5c666d2ae71b8b0c3e4f213  ./crates/gardenfell-nif/src/netimmerse/types/niposkey.rs
1cd2529e898675eeef88c6fcd83381a4185c81a7868ea46e32a854ad52e10a80  ./crates/gardenfell-nif/src/netimmerse/types/niproperty.rs
2dc14d75b597f1f432bbe9ec54b920bb872eef842e89725eba16aa5664e50937  ./crates/gardenfell-nif/src/netimmerse/types/nirenderedcubemap.rs
fcf7ae497d7f968a07e5144d7ccc8bcd947741eacb33eb1435d6fe27f4c6aeda  ./crates/gardenfell-nif/src/netimmerse/types/nirenderedtexture.rs
8501ad1d71710076b6435b24d0047ac69d435d400f37f4309f332a0e89c946b7  ./crates/gardenfell-nif/src/netimmerse/types/nirenderer.rs
507a925d37f5017e542d6dad37b0f5cf100798553538f9f68ea3b837868941ec  ./crates/gardenfell-nif/src/netimmerse/types/nirollcontroller.rs
b32bfeba7bd94ea56493f1d1b7587bb1be36930747bb1be5785d18be44032c8a  ./crates/gardenfell-nif/src/netimmerse/types/nirotatingparticles.rs
302a4cca352b29ea8b5ea14f21db9c52a5d1b747d4fec4af39316c0c9b0ed3fa  ./crates/gardenfell-nif/src/netimmerse/types/nirotatingparticlesdata.rs
ee3000db3df6eb71d8cc341932a93a0677394f2624e7dc00db5ff04df0d5add6  ./crates/gardenfell-nif/src/netimmerse/types/nirotdata.rs
a0a29df7285afb36354fe30717168f5453543a0e71bfb99796f2e991f88edabf  ./crates/gardenfell-nif/src/netimmerse/types/nirotkey.rs
dc8f9cde06c2f76ffb4fa7c542180441cb04c91694e4681814e70535f6dd8136  ./crates/gardenfell-nif/src/netimmerse/types/niscreenpolygon.rs
17d4b2263a873e97c9e5b7b98cd50d91141a5502ff10886c0918c35a380fdb0a  ./crates/gardenfell-nif/src/netimmerse/types/nisequence.rs
087f33f031a878da833dd999b6ee67021091e9ae9c8d8df9dac9d3cb8444f12f  ./crates/gardenfell-nif/src/netimmerse/types/nisequencestreamhelper.rs
59443cac62dd0b261b823cb7f68d6af52b3602a9e8574ef73c3f8fda51a40647  ./crates/gardenfell-nif/src/netimmerse/types/nishadeproperty.rs
33427e32ae3bedb14d980e781ff46b765751b293fc0168120e69e46ab0e8801a  ./crates/gardenfell-nif/src/netimmerse/types/niskindata.rs
da19a67ff3d264b047a3524d3e99389c2ca2496bb842d14a25b678fa39b3aea0  ./crates/gardenfell-nif/src/netimmerse/types/niskininstance.rs
137a2c8f5ae1895f0c2fb7c7c7d6fff271caca7fe0812ed9302cf1cfe2e4c743  ./crates/gardenfell-nif/src/netimmerse/types/niskinpartition.rs
ea47c80ceaaa576ccd4392e4384f32c9ffe503d6c239ee18a96df07d059b5eb4  ./crates/gardenfell-nif/src/netimmerse/types/nisortadjustnode.rs
ccf0c78bdd4d634b0c0fca15aa8bd1ba4fc6e0f0f282c6716aad24533d0397e7  ./crates/gardenfell-nif/src/netimmerse/types/nisourcetexture.rs
d8626e101ec98db3e62b175e790b3a67c5ec5774ffc603279f62fef00171a61b  ./crates/gardenfell-nif/src/netimmerse/types/nispecularproperty.rs
077fa4e6428494c8d6fb203333cda6d4742e764d64fc1d084d801e9016215e9f  ./crates/gardenfell-nif/src/netimmerse/types/nispherebv.rs
6508388e481d151ac6ad25dfad2b5b6a4ab7f457109c2588ae772db739f04739  ./crates/gardenfell-nif/src/netimmerse/types/nisphericalcollider.rs
6b2ffe01fe6555e0ba45801fed83045d7a61fd7c4228818d5335c623f45cb7d9  ./crates/gardenfell-nif/src/netimmerse/types/nispotlight.rs
6af17d429a244b03a67a5d668518458004f8ca3e5ecd031b935ddf5dbf53fa73  ./crates/gardenfell-nif/src/netimmerse/types/nistencilproperty.rs
bc4d501b0a26e74c76e46f252fe398d9b525df46ccbe785865606284292a52a8  ./crates/gardenfell-nif/src/netimmerse/types/nistringextradata.rs
809f4bdc3c3ddea688906c2d22993af8a9c86f053373a15590cbea540527b7cf  ./crates/gardenfell-nif/src/netimmerse/types/niswitchnode.rs
66002b0267076e5d6af853407e9969623a93539a02ed5ceb1e30cbd41eb4b8fe  ./crates/gardenfell-nif/src/netimmerse/types/nitextkeyextradata.rs
b082e81a18bee9c18754bd44063acc7854ac9bd31f466967ea5fc377af9d5ead  ./crates/gardenfell-nif/src/netimmerse/types/nitexture.rs
d6c9fdab8c462ba875b21365ac1a4012b32d249ac18bf3161f7baae7ab69e6d5  ./crates/gardenfell-nif/src/netimmerse/types/nitextureeffect.rs
9331bec2cc97842965e9022ed0c44fb249783565b0cadb02f045c78889b55a29  ./crates/gardenfell-nif/src/netimmerse/types/nitexturingproperty.rs
b9956a6d840c947e19cbdbaa3b4c2be8164c9f77173fb621e7cfba65b9860f63  ./crates/gardenfell-nif/src/netimmerse/types/nitimecontroller.rs
fbfd957a0fc778c68fd53619f00c9207ba97b04c8aae335634043dbcb6a127ab  ./crates/gardenfell-nif/src/netimmerse/types/nitribasedgeom.rs
31f0048414d1f1fdd095c4fc4e97399a3c0cf8b439c812f61d75ce2dbe24d403  ./crates/gardenfell-nif/src/netimmerse/types/nitribasedgeomdata.rs
cfa589724e8f72c26172ba9a15bc62f5d8744909ad6f617d7ed3ce39248499ff  ./crates/gardenfell-nif/src/netimmerse/types/nitrishape.rs
a8c8d8d990de595aa7146d5e9c3a6281d9e733c0c1976b6e7f483f2f329cf60c  ./crates/gardenfell-nif/src/netimmerse/types/nitrishapedata.rs
e3a83c06d485e94c4b1694467fd63ba6d44d8d5b27c695afd5f9f40b79e69790  ./crates/gardenfell-nif/src/netimmerse/types/nitrishapedynamicdata.rs
8c3f29c1ff6618855025f0226801e9b93d58586786ba8ba8602435ab534d1650  ./crates/gardenfell-nif/src/netimmerse/types/nitristrips.rs
aafd58796f3373dc40143fff74f45b0cd4e1b8a091fff77b1d0e1a2b75460ce7  ./crates/gardenfell-nif/src/netimmerse/types/nitristripsdata.rs
a5451c30a237e3b980472db1d844b9b5dfc72ba9daa425e902c0fa8ba77bb8cb  ./crates/gardenfell-nif/src/netimmerse/types/niunionbv.rs
5973a9f33af9bb0731bc0edb0d474b239bcb3aae9bb449acc5d9123bb19fc02d  ./crates/gardenfell-nif/src/netimmerse/types/niuvcontroller.rs
8a4da049fe026f06165cff556d4ce6ee70cce09982ed96cd68c57f036139023b  ./crates/gardenfell-nif/src/netimmerse/types/niuvdata.rs
2e8c10cec2d099c4dad4568b5769af69d95c84302311d5cae61a186d5f4615f1  ./crates/gardenfell-nif/src/netimmerse/types/nivertexcolorproperty.rs
233acef8f08e18cf1b536c64278a440d7c24453a95a9529166ae226eab390b26  ./crates/gardenfell-nif/src/netimmerse/types/nivertweightsextradata.rs
58a51b5cbdfcdf70ad0ab916fe697cb43a3353f401f3b36aeb16ecba37da4ef9  ./crates/gardenfell-nif/src/netimmerse/types/niviscontroller.rs
63b78caf7d5de41c109c80c10f9795630beabce1fdbab7ac5c11d651fdaedde0  ./crates/gardenfell-nif/src/netimmerse/types/nivisdata.rs
26da7d113229343bcd30039e1ac864967f9a9c3b8c88a3e9be57a589fd5397aa  ./crates/gardenfell-nif/src/netimmerse/types/niwireframeproperty.rs
631fe77c23bc218ffd55b172d51dcad93065b4af9b9b846dd611a934dbc78264  ./crates/gardenfell-nif/src/netimmerse/types/nizbufferproperty.rs
7c416f4d3a3f2c152e4418e8b7b931b5b6a529f6e44b66442aff4aeb8e4abdd2  ./crates/gardenfell-nif/src/netimmerse/types/rootcollisionnode.rs
ffb45c6a7f7fadae0287c28c9660b497be064ab0a3f19794482917531c1b9662  ./crates/gardenfell-nif/src/netimmerse/types/tes3objectextradata.rs
f72acf566a941f7bf147d97afb844958e2a471972cc409d96dba9ce357dd0e18  ./crates/gardenfell-nif/src/nif.rs
6289a8312ff16f8d76eed033a619c8e9ddc5ab1b6c430f9e13f48ceed75d0cd3  ./crates/gardenfell-nif/src/nif/skeleton.rs
0818c83e4e4db50f37e22fac46b1f365cf6585863b4c93c176f0419ec1e986ad  ./crates/gardenfell-nif/src/triorder.rs
0eb8ecaec46a7b673e18ed7c4ac2993993d33874632e9c6bfcb87f59866428b3  ./crates/gardenfell-nif/tests/colrules.rs
d8a59cde33051f5d7d47fb4dcebe2be9bdfd1ef794d0afb614ebef7dcb0d2121  ./crates/gardenfell-nif/tests/detail.rs
e30c116ff6bd7cb1f4618a3da8110e70e0c62a5518fddef28fc31ea2fef57f80  ./crates/gardenfell-nif/tests/round18bg.rs
6378b38ff801afdaf4b1ce4e16e8ce60b09d25cf4d6f5ec7dac95d012db492ac  ./crates/gardenfell-nif/tests/round18bh_scan.rs
69d0ead8975448745bbb6db10323914764456f3852bcf461912d62b2f0c675f0  ./crates/gardenfell-nif/tests/round18gi.rs
fd401cc8eb94dead7e264bea723979e81cacb9111c57b6f0e88d2fc9a3b73323  ./crates/gardenfell-nif/tests/round18r.rs
29d43a0c27579ca3d68ec8c5f8b4c4eeb38ac50c24a700918aa0a84867609a5b  ./crates/gardenfell-nif/tests/triorder.rs
386435ec87dc2eaa6ced73b82d825944e330578d98f668a7f5d90e865a029e70  ./crates/gardenfell-tes3/LICENSE
bc925898405c1ecb288d5573e4780a5f386e3797a7924bc37cae5da6f0543d7a  ./crates/gardenfell-tes3/src/esp.rs
a77a00263bf9702768c005734d356634d72022bee9cce156627c95db9c752b9e  ./crates/gardenfell-tes3/src/esppatch.rs
f93f52903fb1d41d5e9709c803f306524cca54ac7b4d8bc727d945333431856d  ./crates/gardenfell-tes3/src/espwrite.rs
286141f551d83bdee639f1d318bca437fafdb8488519044921be46e1be612f31  ./crates/gardenfell-tes3/src/land.rs
39c2358f6ff69ab48aafb4aa0bd652e669dd3129ec1fcda7ea2ab2579adea056  ./crates/gardenfell-tes3/src/lib.rs
5beff5ae80301e70811968b2c9cdb9502c71060693b3c48a5a68751061cd71a0  ./crates/gardenfell-tes3/src/records.rs
ba9c1e867f546093ef0496e0618a0ef2e163ba3a0309098f05817a6a797f938a  ./crates/gardenfell-tes3/src/tes3/bytes.rs
e6548f128aa412ada6486617dd21dc93c70794fbcc853cb5befaf86417290d40  ./crates/gardenfell-tes3/src/tes3/check.rs
9446e417ee024ad6c101c4133c0cef83908690f1c36d98c83e3576ec5318fb7c  ./crates/gardenfell-tes3/src/tes3/mod.rs
353134afc0957b9188e6821722c908b5763f1807cb3454f76c5c3a5de2175430  ./crates/gardenfell-tes3/src/tes3/raw.rs
10aa02580b8be75ca409c8e6ad1d3a8b6754324ce42b81fb0843c48d892fc05b  ./crates/gardenfell-tes3/src/tes3/types/activator.rs
6bbed453923838ff592c65003c1bef9165369e995827f028b38dc0ba6051fe4b  ./crates/gardenfell-tes3/src/tes3/types/aidata.rs
8204c9199d02a1fa02d9b1db89278ad5a22bacd9f0ecbea94b707f8d7ef8fda6  ./crates/gardenfell-tes3/src/tes3/types/aipackage.rs
fd329bec367d13b6da64cef746c7ab4870a2542bc708d70a234d2e91477d33e6  ./crates/gardenfell-tes3/src/tes3/types/alchemy.rs
165dc89fff5bd612d845ac567a91c123db224f90eb5da1957c378c9384d5a0e5  ./crates/gardenfell-tes3/src/tes3/types/apparatus.rs
d90e454fdddac5dcc1dc0b66cf825bcb928a7f5a9488adae2d43e276c5836384  ./crates/gardenfell-tes3/src/tes3/types/armor.rs
176ec7f6ca4cebc9647eada0293c459f7cd2afc188a995ae529948dfa029c6f4  ./crates/gardenfell-tes3/src/tes3/types/bipedobject.rs
19c13c2e633cf437b6ec4d646926e5cb3ff4123aab9c307d41f81b76979f4a8b  ./crates/gardenfell-tes3/src/tes3/types/birthsign.rs
2cd08388204f1c4810e50b4f9365c38e810424cdf3a10a02e44f2dfadd886594  ./crates/gardenfell-tes3/src/tes3/types/bodypart.rs
a165640f21f260936ee748ede8e5da9f2a165b423dd3a7375899ab1d61879d42  ./crates/gardenfell-tes3/src/tes3/types/book.rs
5f073270da7b182da1a73032d7d0d10376e402d0dca0464bd54af393acd9258b  ./crates/gardenfell-tes3/src/tes3/types/cell.rs
c61859ec668267dc88db4c776f2165e7112db128f8f4b011f54581f393178c82  ./crates/gardenfell-tes3/src/tes3/types/class.rs
69a318c6acfdd42a1299a877dbaabae87855ad51f738d0ba969d1e8aeb2a65f5  ./crates/gardenfell-tes3/src/tes3/types/clothing.rs
94a0b4d3a8faa88910f8855f4282b6ba82930883aee3405dc9ab7769b0ee995f  ./crates/gardenfell-tes3/src/tes3/types/consts.rs
09ceda149aa163f6a435e36741bc64f3529819c9da3fe09fb2b633e4fc157af1  ./crates/gardenfell-tes3/src/tes3/types/container.rs
5af0189ad15f48d25120ccbc55052ae0a7db1def426f14bf38ea74801286ed85  ./crates/gardenfell-tes3/src/tes3/types/creature.rs
d6cbbdfe3f04b584b7f5f168ae66072c5b76bd713657abfc5473273177af7f12  ./crates/gardenfell-tes3/src/tes3/types/dialogue.rs
1ee78a6b87a60d08b3d874578f5b54ba0686c810d989ee098b29c489ccea8c1d  ./crates/gardenfell-tes3/src/tes3/types/dialogueinfo.rs
ed4bd49def1f7e654ce6e945d8a5dbb28f23c39211fdfb9627f6112f8e89e82a  ./crates/gardenfell-tes3/src/tes3/types/door.rs
1c672781e2203608e81cb06ee8ddcbf0d35abf9f85f43f552564cc872197376b  ./crates/gardenfell-tes3/src/tes3/types/effect.rs
e705d3d7ac9ecd6f97a1e60be14004d6d9eb874ca1a1fc648653294058b7c6a8  ./crates/gardenfell-tes3/src/tes3/types/enchanting.rs
1faeeeb9fb7fa029f58b0c44b5951220ee740759265f07373a6650843d43a29f  ./crates/gardenfell-tes3/src/tes3/types/faction.rs
fa3ea66b80c4500b9f832a5c21e8cd1648ede58bb35135c57dc3f49b1075a8f5  ./crates/gardenfell-tes3/src/tes3/types/gamesetting.rs
28558e358b400242ed949ab94f3d23b382a712fce4fd5fe987486d6c8c086b43  ./crates/gardenfell-tes3/src/tes3/types/globalvariable.rs
97fd26b699f2660c15594f6cb9d359df3666f1c331d73dfe1ecf3ff6c92a945f  ./crates/gardenfell-tes3/src/tes3/types/header.rs
b5e48f93c9125fe89f7aa6372163daa91ca2640392364a660b090a455c767581  ./crates/gardenfell-tes3/src/tes3/types/ingredient.rs
b698c5ff979ddd4a7e191c467caa4616d48f8e978f927c173ab2dcf40d1dae82  ./crates/gardenfell-tes3/src/tes3/types/landscape.rs
b435fa72a24d64f8d917f1d014488e92292193b9289b087b3a0def03572ceecc  ./crates/gardenfell-tes3/src/tes3/types/landscapetexture.rs
45907f30a36f963b5914379c6ea5784bddb9b3fc49343484e8c76d8145a2feaf  ./crates/gardenfell-tes3/src/tes3/types/leveledcreature.rs
1634ac043dd44a5963d3772bcd1b771706fd74d0cc46395aa808e340efc74237  ./crates/gardenfell-tes3/src/tes3/types/leveleditem.rs
06530da5673b6f91985b11eb30ce8099f31d6eea74c5b34ead69b3f4221ce4c8  ./crates/gardenfell-tes3/src/tes3/types/light.rs
80e80e1e874c47ee963b1b30848513043121e54f1f1c05d3a6c31512a61f9122  ./crates/gardenfell-tes3/src/tes3/types/lockpick.rs
1176befb3abbe73c58b0e93c126bef338ede1149cff7b83c159c91a977745e93  ./crates/gardenfell-tes3/src/tes3/types/magiceffect.rs
3d6946c55947aa819fb38fefc3628c23bbfa9d08d580f37a0cc1119a6c93a86b  ./crates/gardenfell-tes3/src/tes3/types/miscitem.rs
e38b9ed91598b2f0f8a0cf66eca01a958b9f57b64172f436382ab514475a3907  ./crates/gardenfell-tes3/src/tes3/types/mod.rs
aea4d6d4364e6ebb41a0862fab8d6f149eea75e7885c5f53155ccf72e9b1a29c  ./crates/gardenfell-tes3/src/tes3/types/npc.rs
7eb758e6d055491317481637242525c952eaca88e65c71fa5c159fb4aa50992c  ./crates/gardenfell-tes3/src/tes3/types/pathgrid.rs
101862b10949ce7aac8777a7b6f1be6cc71b3d8505a81fb404881127f7b3212c  ./crates/gardenfell-tes3/src/tes3/types/probe.rs
ee926edc7572d8139b6128e6921cc19002fe66e3ff0e5c1eab9be2cb6887dd4c  ./crates/gardenfell-tes3/src/tes3/types/race.rs
2a7fa504f6124935f4d3f9732da1c3cac2752707fa06332170bffb18155dac12  ./crates/gardenfell-tes3/src/tes3/types/reference.rs
3e4b386231f16bf17469e8483548438d37a3b3d572564016378e44f0fcd1574a  ./crates/gardenfell-tes3/src/tes3/types/region.rs
ae648594a2ad5aef88275e81ebaed541b180595ae56939a7f6f7aa2cf2b08395  ./crates/gardenfell-tes3/src/tes3/types/repairitem.rs
aac1d762a89d3dfa9c1949fc0ec0540372eddc6a430e424bd85588bf0683f77c  ./crates/gardenfell-tes3/src/tes3/types/script.rs
b2960f929ffb1a8add3dab4427e4f0efc681115425f4e1f2c2b87bb5c0a6861c  ./crates/gardenfell-tes3/src/tes3/types/skill.rs
89d41643536a3ec3c2147c07c83070dddc9d0476ceb726ed04af10e66f51d48d  ./crates/gardenfell-tes3/src/tes3/types/sound.rs
72d556908c3d97831b8d0b3019d4f2d1bc37130bbbb4da1bf095743628ca7bad  ./crates/gardenfell-tes3/src/tes3/types/soundgen.rs
463408efebd713014113ab4625a4ade5bfd0a9a5b409b2a30fbdf298723cad3d  ./crates/gardenfell-tes3/src/tes3/types/spell.rs
ee9cc6a61574d06fbce191f3ae28227efe63ccab9a592d823e713e6f83088f58  ./crates/gardenfell-tes3/src/tes3/types/startscript.rs
5585d5d05639c6247e012ca2f4044c6bcc0efc1611a458407f6f5d5517393c2a  ./crates/gardenfell-tes3/src/tes3/types/static_.rs
c63f33c515ddeb95d2fb19a4d1b18e4ac71488aa8a9a63e32be32619d76ac173  ./crates/gardenfell-tes3/src/tes3/types/weapon.rs
386435ec87dc2eaa6ced73b82d825944e330578d98f668a7f5d90e865a029e70  ./crates/gardenfell-text/LICENSE
e23d30be5b5bef70e286bd8d452db300264166646863f883e55805e3bbda1f7d  ./crates/gardenfell-text/src/lib.rs
6a3b1516bb8d9915760b872d4590a9c72a6c0061bd14b92e85679e0975fd7127  ./crates/gardenfell-text/src/text.rs
386435ec87dc2eaa6ced73b82d825944e330578d98f668a7f5d90e865a029e70  ./crates/gardenfell-world/LICENSE
98f1a0662d7875d6988490cf0094c20c9de419ea2dbc1c2346e42378323f0821  ./crates/gardenfell-world/examples/attachbox.rs
1da4556a667b8972f7fa87e38675c118d40f6ca9de62d60aca8983cb438699ed  ./crates/gardenfell-world/examples/attachchain.rs
78c96889729c0315733f6477a937da641e2c67e4c5a600be0edfb256c0bf9800  ./crates/gardenfell-world/examples/attachoff.rs
b18f596826656d35ed7db7ca497a78f6a258e6b065e22a46e2801ec8e8bd3855  ./crates/gardenfell-world/examples/drawmodes.rs
b50d5519d8f84e75f6c13d8905916675211a2c0b297de6b9c1643606af6656c7  ./crates/gardenfell-world/examples/findref.rs
8eec4e1014d8050bb5b782066c3b424ae6e2cb56f2cef334407326397a7b90bd  ./crates/gardenfell-world/examples/lightrec.rs
c8bf4ced9cd1d714417e350c5ee598dcfd3e9fb9cbdc848d7eac5881f10e7441  ./crates/gardenfell-world/examples/lights.rs
cd818520480df52f8da3792a48e52ee201f4ce63f9768d0af3212360e12ed4a7  ./crates/gardenfell-world/examples/mapcost.rs
6587a8709d92eaa6c04b36405f106594974d7b2853a292ef295851ebf6614c4d  ./crates/gardenfell-world/examples/maphi.rs
1b0336dcffb29ba07b86e61c7ae976215ffdd600f5dd8129ae7cb269915d1d6b  ./crates/gardenfell-world/examples/whatis.rs
3ba421467ae8f6ea552a21d5dce4fe52f3de3ccdd94efd05806bd6b7ab5f825d  ./crates/gardenfell-world/src/coverage.rs
087ba8464ab0225607481ec62f675a640b6c26166297df994fcc32eee13c1ebf  ./crates/gardenfell-world/src/lib.rs
a3e3052e7d9889cc6af68c489a51971e27bcb3d03f0a4ed7568b8099e23a9f0c  ./crates/gardenfell-world/src/objects.rs
e51c338eb68dca06779d18d58c719e27a25ee435b58f9a0becff5247c393d9b5  ./crates/gardenfell-world/src/spans.rs
75f89b16e5c0f6f00b3c3c725e4288d0ec7ca7b8a08a164065f07965e1c8e921  ./crates/gardenfell-world/src/terrain.rs
61465efe3777906bc532286df087a10703f0d1d79eb3d026b65a6c0c504699ba  ./crates/gardenfell-world/src/world.rs
5876469a5664ecde75b497a4f8f561822038f6220052a540b953fe686f456cd6  ./crates/gardenfell-world/src/worldmap.rs
4bea1a576800a07f224a4b8fe0ad80ef7c73b0e163680c2d794617de5d4feb70  ./crates/gardenfell-world/tests/bsa.rs
0dfa4d9044b8d52021083ec1e919185e02fc61df05624ca482e0aca962966311  ./crates/gardenfell-world/tests/coverage.rs
4e1201e7442d57c94895a217522a76a38087e2c84c7ed0253e186cdf8a0a9784  ./crates/gardenfell-world/tests/lights.rs
8779a22148bbf63f1cc6d42a39f2be91fd92dda9b38e56739fc1ceb6b01995e5  ./crates/gardenfell-world/tests/ltex.rs
593cc610b1dc0c6263445efb2cd126a4b8918d5a28b4f753c24fb4321ab60d37  ./crates/gardenfell-world/tests/round18ab.rs
ce452402f44654de81a8d15cf41a4ab1f301f2c8a49ba2f977ada99dea30218b  ./crates/gardenfell-world/tests/round18bl.rs
a7fbe4eecdb2298060d17ac03539c673a438adb3cf3d7b28af8583c33473fb8e  ./crates/gardenfell-world/tests/round18cr.rs
8fbce81fe2c6f4f0a5187d31e932f5a50298dab905afabfaa3cf06ea2155cff3  ./crates/gardenfell-world/tests/round18ek.rs
9d7811acca7b39b0b78f53d23c61eb818c29b81ff09e8de2ec3216fdf00bf8c8  ./crates/gardenfell-world/tests/round18gf.rs
52cbcb24d67149527606484575ea421591d84d8e9c0160b397f0d858b9811116  ./crates/gardenfell-world/tests/round18gg.rs
9701e6c73c90c24afafa4a0a289e2def37442acd686219847be2ec868ea97ce4  ./crates/gardenfell-world/tests/round18gh.rs
8bf071b88842c9fe562d11e3abb13bbf214be1566a47a75d3e50d0c02589fdbd  ./crates/gardenfell-world/tests/round18gn.rs
4cea276da50fd634788dd807a8d03bd81e56c388f109dc183f2230aa8b05ae26  ./crates/gardenfell-world/tests/round18gq.rs
409e6b9ad6a2cfc4697d65409c70ae41f4052d5ddbb547bd178d311b919a3eb2  ./crates/gardenfell-world/tests/skin.rs
591d301c56fb5630febb263bcba2a3611385cd34b20e0e678c0c52ce10c5ce8e  ./crates/gardenfell-world/tests/wraithguard.rs
056e4236cf6e5edd09264f3049149a9b093a13c47943d48d921a8fa443b366f5  ./languages/gardenfell.en.toml
f78d123dedd626ccace2be6b542c185a094618f1d9dc9404d1b2a4910452ab94  ./page-source/catalog.json
```
