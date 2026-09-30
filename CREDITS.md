# Credits & Acknowledgements

Wraithguard Toolkit stands on the work of a lot of other people. This tool exists
because these projects were generous enough to share their code, formats, and
research. Huge thanks to everyone below - the good ideas are theirs; any bugs are
ours.

If you are one of these authors and want your attribution changed (or removed),
please get in touch and we'll fix it right away.

---

## Code ported or adapted from (MIT-licensed)

These projects are MIT-licensed. We ported logic and/or cross-referenced their
implementations; their copyright notices are reproduced with the relevant parts
and their `LICENSE` files are included in their source folders in this repo.

- **mlox** - © 2009–2017 John Moonsugar (alias), dragon32, Arthur Moore. MIT.
  The load-order rule engine and the rule databases (`mlox_base.txt` /
  `mlox_user.txt`) this whole tool is built around. Our matching, ordering, and
  `[Conflict]/[Requires]/[Note]` predicate logic is a port of mlox's. Several
  of our **Lint** checks (evil GMSTs, the interior fog-density-0 bug,
  expansion-function dependencies) come from mlox's `tes3lint`, credited
  separately below. (The missing-pathgrid check does *not* come from mlox -
  see the unlicensed-scripts section.)
- **tes3lint** - © 2009 John Moonsugar. MIT. Distributed as part of mlox.
  A diagnostic tool for TES3 plugins. Our **Lint** feature reimplements
  its useful checks against plugin binaries (so they see the whole OpenMW
  multi-folder VFS, with no Perl needed); since 4.2.0 the record scan behind them
  runs in Rust (`native/src/lint.rs`). One thing is *reproduced rather than
  reimplemented*: the table of **72 "evil GMSTs"** - the exact name/value pairs
  an old Construction Set wrote when run without both expansions. Those values
  are research, not something we could rederive, and the table carries John
  Moonsugar's copyright notice inline at `_EVIL_GMSTS` in the engine.
- **[mlox-rules](https://github.com/DanaePlays/mlox-rules)** - maintained by
  DanaePlays and contributors. The **actively-updated** rule database that
  modern mlox (v1.1+) and plox both use. Our "Update Rules..." button downloads
  the current `mlox_base.txt`/`mlox_user.txt` from this repo.
- **plox** - © 2024 Moritz Baron. MIT.
  A Rust reimplementation of mlox. Used as a second reference to harden our
  engine (wildcard/`<VER>` matching, order transitivity, predicate functions).
- **tes3conv** - © 2025 Greatness7. MIT.
  Converts Morrowind plugins ↔ JSON. It was the record-identification and
  field-diff engine behind Check Conflicts, and the MOMW Tools Pack shipped it for
  this tool. Since 4.2.0 the same work is done in process by the `tes3` crate
  (below), and tes3conv is only a fallback when the Rust module is missing. Its
  JSON layout lives on: the records the toolkit works with still take tes3conv's
  shape, field names and all.
- **tes3** ([`Greatness7/tes3`](https://github.com/Greatness7/tes3)) - © Greatness7. MIT.
  The Rust library of TES3 record and NIF block types, and now a dependency:
  `native/` (the `wraithguard_native` Python module) reads and writes plugins on
  `tes3::esp`, reads meshes on `tes3::nif` (the Resource Conflicts summary, the
  mesh viewer's block panel and field editor), and reads `.bsa` archives on
  `tes3::bsa`. Our `wraithguard/esp/` keeps its record dataclasses - modelled on
  `libs/esp/src/types`, with the crate's enums and flags - as the objects the
  toolkit works with, built from the crate's JSON. The Python ports of its record
  readers and writers, and our own NIF reader (below), have been retired. The cell
  viewer's engine (`viewer-shell/viewcore`) also reads archives, meshes and the
  plugin records its world is built from with the crate, and falls back to its own
  readers only for a file or record the crate refuses.
- **Merged Lands** - © 2022 David Von Derau. MIT. Licence text vendored at
  `License/MergedLands/LICENSE`.
  A Rust tool that merges the landscape changes a load order would otherwise
  discard. Our `wraithguard/land/` package is a **port of it**, function by
  function - the reference landmass, the per-mod diff, the four conflict
  strategies and their thresholds, corner-then-edge seam repair, cleaning, the
  shared `LTEX` index space, the conflict images, and the `.mergedlands.toml`
  sidecar schema, which we read with the same field names and values so
  settings written for the original work here unchanged.

  Since 4.2.0 the per-vertex core of that port - the relative grids, the
  per-vertex merge in every strategy, the slope limiter, normals and height
  decoding - runs in Rust (`native/src/land.rs`), translated from our Python
  rather than from the original, with our fixes kept; settings, reports and UI
  stay in Python.

  `MERGED_LANDS.md` accounts for **all 191 functions** in its
  `src/`, names where each one lives here, and records every place we diverge
  and why. Our additions - a slope limiter, curvature-weighted resolution, and
  digest-based cleaning - are marked as ours there rather than attributed to it.

- **Merged Lands, OpenMW fork** (`OpenMWMergedLands`) - MIT, retaining © 2022
  David Von Derau; licence identical to the original's, `License/MergedLands/LICENSE`.
  The OpenMW-oriented continuation of Merged Lands (reads `openmw.cfg`, emits
  `.omwaddon`, app-config `merged_lands.toml`). We compared our port against it
  file by file - the *fork comparison* section of `MERGED_LANDS.md` records the
  result: it is a modernisation and OpenMW-integration pass, not an
  algorithm-fix pass. From it we adopted the default texture-fallback behaviour
  (`fallback_texture_index`: substitute the smallest valid painted texture for
  an unresolvable index so the plugin always loads, while still reporting the
  substitution), and it confirmed the `.mergedlands.toml` schema is unchanged
  from the original.

- **Gardenfell / GrassForge** - © 2026 Robin Hjelte. **MIT**; its page is
  **GPL-2.0** because it carries MGE XE shader ports. Shared with us by its author
  to build on. **The cell viewer and the mesh viewer are Gardenfell's viewer**,
  brought into this repo and adapted - its code, not an imitation of it:
  - **The engine**, `viewer-shell/viewcore` (MIT, Robin's notice kept): the
    install's files and archives, the load order merged into a world, meshes and
    textures read for drawing, the sky and weather.
  - **The command layer**, `viewer-shell/src/cellviewer/commands.rs` (MIT): what
    the page calls.
  - **The page**, `viewer-shell/ui` (**GPL-2.0**): the WebGL2 renderer, terrain,
    water, sky, particles, shadows, ambient occlusion, depth of field, underwater,
    the cell picker and its map. The MGE XE shader ports (shadows, fog/tonemap,
    water, sunshafts, underwater, SSAO) are why this part is GPL. It is a separate
    frontend loaded into the webview, so the GPL does not reach the MIT code. See
    `viewer-shell/ui/LICENSE`, `License/Gardenfell` and `License/MGE-XE`.

  What we changed: stripped it to a viewer (grass generation, painting, the
  statics setup, ESP export and the profile/rules managers are gone from the page,
  the commands and the engine); it opens Wraithguard's own setup (the sort panels'
  current state); the engine reads with greatness7's `tes3` crates and keeps its
  own readers as the fallback; and ours are the ORI inspector, `groundcover=`
  loading, the mod-coverage map in the cell picker, mod highlighting, the viewport
  settings, the mesh-viewer mode with its block editor, and the water changes
  credited at the end of this file. `viewer-shell/MANIFEST.md` lists every part.

  Two smaller things came from it earlier:
  - Its tiny hand-crafted animation test NIFs (`morph.nif`, `anim.nif`,
    `uvsets.nif`, `particle_move.nif`, `flap.nif`/`.kf`) are vendored under
    `tests/fixtures/gardenfell_anim/` (MIT). They first tested our own NIF
    reader's animation extraction, where they caught two bugs -- geometry lost
    under a `NiCollisionSwitch`, and a particle `NiTriShape` drawn as a surface --
    and now test the crate-based mesh readers in `native/` and
    `viewer-shell/viewcore`.
  - Its start-up discipline -- *read a thing once and reuse it rather than
    re-reading it every pass* -- shaped our first, Python cell previewer, before
    that previewer was replaced by Gardenfell's own.

- **momw-configurator** - © Modding-OpenMW.com (johnnyhostile). MIT.
  We read its `cfg/custom.go` to reimplement its customization-apply logic
  faithfully, so the **Export preview** can simulate exactly what the
  Configurator will do to your `openmw.cfg` (matching, insert/replace/remove
  order, ambiguity errors) before it runs.
- **modmapper** - © 2023 Michiel. MIT.
  The inspiration and reference for the cell-map heatmap (which mods touch which
  exterior/interior cells).
- **Tes3EditX** - © 2023 Moritz Baron. MIT.
  Referenced for TES3 record handling and conflict-resolution UX.
- **TES3Tool** - © 2019 SaintBahamut. MIT.
  Referenced for the TES3 binary record/subrecord layout used by our built-in
  parser.
- **MWEdit** - © Dave Humphrey and contributors. MIT.
  Its `data/Functions.dat` and `mwedit/script_defs.h` are the primary source
  for our script-bytecode opcode table: the function names, their opcode
  values, and the parameter-flag words that describe each operand's encoding.
  Without it the **Bytecode** view in the diff window would be guesswork -
  and guesswork is exactly what we refused to ship.

  The compiler-internal opcodes that no function table lists (notably
  `_SetReference`, emitted for `id->Func`) were **measured from a corpus of
  real compiled scripts** rather than taken from anyone's source - an opcode's
  numeric value is a fact about the game's own data files. `tools/gen_opcodes.py`
  regenerates the table and documents each derivation.
- **Morrowind Dialog Explorer (MWDE)** - © 2018 Sophie Kirschner
  ([pineapplemachine.com/files/mwde](https://pineapplemachine.com/files/mwde)).
  MIT. The **"Read as dialogue"** view (`wraithguard/tes3fields/dialogue.py`)
  turns a DIAL/INFO record into the line an NPC says, to whom, and under what
  conditions. The condition phrasing - the mapping from a SCVR filter's
  type/function/comparison to an English "If ..." line, the boolean special
  cases ("is not dead", "is a member of faction X"), the speaker-context
  assembly - and the result-script token lexer (`script_tokens`, from MWDE's
  `src/syntax_highlight.py`) are adapted from MWDE's `src/info_string.py`,
  retargeted from its raw-subrecord model onto the enum names tes3conv emits.
  The **function labels themselves** follow the `tes3` crate's own
  `FilterFunction` names (prettified) rather than MWDE's table, so they stay in
  step with the JSON the tool actually reads. MWDE's wider record/field schema
  (`record_types.py`, `sub_record_field_types.py`) was reviewed but not ported:
  our `tes3fields/schema.py` already covers a superset of its records, and its
  byte-level field readers duplicate what tes3conv/our native reader do.

## abot's tes3cmd scripts (idea credited, no code used)

- **`missing_pathgrids.pl`** and **`cell_conflicts.pl`** - © **abot**.
  Published as *Missing Pathgrids* and *Cell Conflicts* on abot's own site,
  ["Morrowind is Home"](https://abitoftaste.modlist.x10.mx/morrowind/index.php?option=downloads&catid=58&Itemid=50&-Morrowind-tools)
  (Downloads → Morrowind tools), alongside MMOG, MRS and abot's other tools.

abot is behind a great deal of what makes Morrowind still worth playing -
Water Life, Silt Striders, the merged-object and resource-scanning tools that
half the community's load orders depend on. These two `tes3cmd --program-file`
scripts are small by comparison and easy to overlook, so: thank you.

**The files themselves carry no copyright line and no licence text.** No
licence granted means the author keeps all rights, which makes these the most
restricted inputs in this project rather than the least. So we treated them as
*read-only inspiration*: **no line of either script is in this tool**, and both
of our implementations were written from scratch in Python against plugin
binaries. What we took is the diagnostic idea, which copyright does not cover.

- *Missing Pathgrids* - the idea: an interior cell with no `PGRD` record is a
  bug, because NPCs cannot pathfind there. Our `[NO PATHGRID]` check
  deliberately *diverges*: the original only considers plugins earlier in the
  load order and so reports false positives, whereas ours accepts a pathgrid
  contributed by **any** plugin.
- *Cell Conflicts* - the idea: "show me every mod touching the same cells as
  this one." That became the **Focus on mod** filter in our cell map (whose
  implementation follows *modmapper*, MIT).

abot - if you would rather we credit this differently, drop the mention, or not
reference your scripts at all, say the word and it is done.

## Approach referenced (no code copied)

- **TES3 Conflictsolver Editor** - ©2026 kirgan 
  (a Mini-TES3Edit–style patch tool). No license file is
  distributed with it; **no code was copied**. We credit it for the field-level
  record-diff *approach* that inspired our field comparison view. All rights
  remain with its author.
- **xEdit / TES5Edit / SSEEdit** - © the xEdit team. **MPL 1.1; no code copied.**
  Our conflict-colour convention -- a record's overall status colours the row
  **background**, what one plugin does colours the **text** -- is xEdit's, the
  one every modder already reads (green = no conflict, yellow = benign override,
  red = a losing edit; see the STEP guide's colour legend). The specific hues in
  `wraithguard/gui/conflict_colors.py` are the Material-Design values from a
  community *Material dark-mode edit of xEdit's `xEdit.ini`* (its
  `[ColorConflictThis]`/`[ColorConflictAll]` sections, Delphi `TColor` integers
  we decoded to `#RRGGBB`). Only the colour **values and the scheme** were
  taken -- data, not code. Material Design is Google's, under Apache-2.0.

## Our own NIF reader (retired), and why it was written rather than imported

*Retired: meshes are now read by greatness7's `tes3::nif` (see **tes3** above). This
section and `NIF_PROVENANCE.md` are kept as the record of the reader that was.*

Until 4.2.0, `wraithguard/nif/` read Morrowind meshes with our own code. That
was a licence decision, taken deliberately and recorded here so it is not
revisited by accident:

- **pyFFI** - LGPL. The obvious Python choice. This tool ships as a PyInstaller
  onefile binary, and statically bundling an LGPL library carries a relinking
  obligation that does not fit that distribution.
- **nifly** - GPL-3.0. **io_scene_mw** (Greatness7's Morrowind Blender plugin) -
  GPL-3.0, Python, and scoped to exactly this game, which makes it the most
  tempting option by far. **NifSkope** - GPL.
- **nif.xml** (the NifTools format description) - in a GPL-3.0 repository whose
  own licence status is [disputed upstream](https://github.com/niftools/nifxml/issues/86).
  An unresolved licence is worse than one that clearly says no, so it was not
  used as a source either.
- **niflib** - **BSD-3**, and therefore the permissively-licensed reference to
  consult if a layout ever needs checking against an implementation.

### Greatness7 relicensed the `es3` library so this project could use it

On 28 July 2026, **Greatness7** - author of the Morrowind Blender Plugin and of
`Greatness7/tes3` - offered to relicense the NIF library inside `io_scene_mw`,
and then did it:
[`cbe18b5`](https://github.com/Greatness7/io_scene_mw/commit/cbe18b558299e14ecd959183e3cf9ea096fe95df)
adds an MIT `LICENSE` to `lib/es3/`. `Greatness7/tes3` was already MIT.

That was an unprompted act of generosity toward a project that had spent months
carefully working around his code, and it is worth naming plainly. The
relicensed library is `lib/es3/` **only**; the rest of `io_scene_mw` remains
GPL-3.0 because Blender requires plugins to be, and this project respects that
line. See `NIF_PROVENANCE.md` for the exact boundary.

Within an hour of reading `tes3`, the cross-check had confirmed this project's
hardest-won layout - the typed bounding box - and found a gap it could not have
found alone: `NiUnionBV`, a bound type no file in either corpus carries, which
this reader would have refused. A second implementation sees what a corpus
cannot.

**How each field layout was actually derived - and what was deliberately not
read to derive it - is recorded in `NIF_PROVENANCE.md`.** That document is the
companion to this one: this section says *why* the reader was ours, and that one
says *where every fact in it came from*, with the worked derivations so they can
be re-run rather than taken on trust.

Going GPL-3.0 was considered seriously: it would unlock io_scene_mw, pyFFI,
nifly and - the bigger prize - **OpenMW**, whose NIF loading, texture handling
and `openmw.cfg` semantics this tool models from the outside. The project stays
**MIT** for now, so none of those were read for the reader's field layouts. They
came from the publicly documented format, checked against real meshes with
`tools/check_nif_layouts.py` (retired along with the reader).

## three.js - bundled, not merely referenced

`wraithguard/viz/assets/three.cjs` is **three.js r186**, MIT licensed, with its
licence text beside it as `three-LICENSE.txt`. It is the first third-party
*source* this project ships, as distinct from the Python packages PyInstaller
already collects, so it is called out here rather than left to be discovered in
a build.

It is a **CommonJS** file, which looks like an odd choice until the constraint
is stated: modern three.js ships ESM only, split across `three.module.js` and
`three.core.js`, and **ES module scripts do not load from `file://`** - the
origin is `null` and the CORS check fails. The viewer pages are written to disk
and opened in a browser, so no ESM packaging can work. A single CommonJS file
with no `require()` of its own runs as a classic script behind a three-line
shim, which is what these pages need.

Through r185 three.js shipped exactly such a file as `build/three.cjs` and we
vendored it unmodified. **r186 removed it** - `build/three.cjs` is now a stub
that `require()`s the ESM module - so the vendored file is no longer upstream's
own build. It is built by `tools/build_three_cjs.py`, which concatenates
upstream's unmodified `three.module.js` graph into one `module.exports` with
esbuild: packaging only, no minify and no source transform. The provenance is
therefore a command anyone can rerun, not a binary to take on trust.

The orbit controls in the page are ours, not three.js's `OrbitControls.js`,
because that imports the bare specifier `'three'` and would pull ESM back into
a page built specifically to avoid it.

## Archives and textures, and why they are ours

Archives went the same way as meshes. Since 4.2.0 they are read by `tes3::bsa` (through
`wraithguard_native.Archive`); before that, `wraithguard/nif/bsa.py` read them
with our own code, for the reasons that follow. **bethesda-structs** (MIT, Stephen Bunn) via **BSAFileExtractor**
(MIT, Pierre GAMBIER) would have been licence-compatible, so this was an
engineering call rather than a legal one: it pulls in `construct`, `multidict`,
`attrs` and `lz4` - the last with a compiled extension - ships a 49 MB tree
covering Fallout and Skyrim record formats this project will never touch, and
every archive in its own test suite is the *post-Morrowind* BSA format, which
shares an extension with Morrowind's and nothing else. Morrowind's layout is a
header and three tables. Neither project was read for the format; ours was
implemented from the public description and checked against a shipped archive
with `tools/check_bsa.py`, which still checks the crate-based reader today.

`wraithguard/images/` is ours for the same reasons and by the same method. Since
4.2.0 its per-pixel decoders (DXT1-5, BC4, BC5, BC7, uncompressed DDS, Targa)
run in Rust (`native/src/img.rs`), a step-for-step translation of our Python
decoders, kept pixel-identical to them by `tests/test_images_native_parity.py`;
the history below is of the decoders themselves.
Pillow would decode these textures, but it is a large binary dependency in a
PyInstaller onefile build. It was used instead as an **oracle**: the corpus
textures decode byte-for-byte identically to it, BC7 matches on 19,380 random
blocks across every mode and partition, and every PNG we write reads back
through it unchanged. Used but not read, exactly as with NifSkope.

**`pydds` was evaluated for BC7 and rejected on licence.** It is the closest
technical fit - DDS decompression bindings including BC7, which is precisely
what was wanted - and it is **GPLv3-or-later**, which would relicense this
entire project. That decision needed no technical argument at all. Two further
facts made it moot anyway: it *depends on* Pillow rather than replacing it, so
adopting it would have added a dependency rather than removed one; and it is a
compiled extension at version 0.0.8, marked alpha.

`quicktex` (Apache-2.0) would have been licence-compatible and was the option
if a hand-written BC7 decoder ever proved too slow. It was not needed: ours
matches an independent implementation exactly, and moving it to Rust took a
2048px BC7 texture from about 12 s to about 70 ms.

The BC7 tables come from the **published format specification** - Khronos's
OpenGL BPTC specification and Microsoft's Direct3D 11 documentation - not from
any implementation. `NIF_PROVENANCE.md` records how that was verified, and why
transcribing six hundred numbers needed a cross-check rather than a unit test.

## Referenced for formats & behavior (GPL - no source copied)

We read these projects to understand file formats and expected behavior. **No
GPL source was copied into this tool's MIT code**, so no copyleft obligations
attach to it (the one GPL part, the cell viewer's page, is its own GPL-2.0 silo -
see *The cell viewer page (GPL-2.0)*);
the credit is one of gratitude and correctness.

- **OpenMW** - GPLv3. The engine that makes modern Morrowind modding possible.
  Referenced for `openmw.cfg` semantics, the `.omwaddon`/`.omwscripts` Lua
  formats, and VFS (`data=`) resolution rules. Also read for the *math* two
  animation controllers apply -- `NiUVController`'s offset/tiling and
  `NiGeomMorpherController`'s `base + Σ weightᵢ·deltaᵢ` vertex blend
  (`components/nifosg/controller.cpp`) -- which is format behaviour, not code:
  our cell-viewer playback is an independent implementation.
- **Mod Organizer 2** - GPLv3. Referenced for the "Data" loose-file conflict
  concept behind our data-path (VFS) resource conflict checker.
- **MWSE** - © NullCascade, Merzasphor, Greatness7 and contributors. **GPLv2.**
  We read `MWSE/OpCodes.h` to *check* our opcode table and found it agreed with
  MWEdit on all 533 opcodes they share. Because MWSE is copyleft and this tool
  is not, **no MWSE source was copied**: the shipped table is built from MWEdit
  (MIT), our own corpus measurements, and `customfunctions.dat`.

  That last file needs saying precisely. `customfunctions.dat` is a **data file
  in MWEdit's own text format**, describing the MWSE / MW-Enhanced script
  functions so MWEdit can compile against them. It is installed by running the
  MWSE updater rather than being part of the MWSE source tree, and it is
  configuration for an MIT tool, not a copyleft header - so reading it does not
  bring GPLv2 obligations with it. It contributes **360 opcodes** the base game
  has no equivalent for, which is what makes an MWSE-scripted mod disassemble
  instead of coming out as raw bytes.

  It spells parameter types symbolically (`Long | String`) where
  `Functions.dat` uses hex flag words, and the mapping between the two was
  **derived, not copied**: the two files describe 106 of the same functions, so
  correlating those pins each symbolic name to exactly one bit value. The result
  matches the `FLAG_*` constants already taken from MWEdit's MIT header, which
  is how we know the derivation is right.

  Where the two files disagree - two renames (`XDrop`/`XDropItem`,
  `XEquip`/`XEquipItem`) and 26 differing operand shapes - **the existing
  MWEdit-derived entry is kept** and the disagreement printed rather than
  resolved silently. 25 of the 26 are the same call: MWEdit says `String` where
  customfunctions says `Long | String` for a filename or object id, and UESP's
  per-function pages document those parameters as strings, which settles it.

  The 26th is a genuine error and is corrected: MWEdit gives
  `XFileWriteFloat` a single float operand with no filename, where
  customfunctions lists two and UESP documents
  `xFileWriteFloat filename (string), value (float)`. Corrections live in one
  small explicit table in the generator, each with its evidence.
- **MGE XE** - GPL-2.0. Read alongside MWSE for the same opcode cross-check; no
  MGE XE source is in the MIT code. Its **shaders are ported into the cell viewer's
  page**, which is GPL-2.0 for exactly that reason - see *The cell viewer page
  (GPL-2.0)* below.
- **Wrye Mash** (Polemos fork) - © 2017-2021 Polemos, based on Yacoby
  (2011-2016), Melchor (2009-2011) and Wrye (2005-2009). **GPLv2-or-later.**
  Wrye Mash popularised the *features* of removing a master from a plugin and
  re-pointing one at a renamed file ("Change to.."), and we read its `mosh.py`
  (`FileRefs.remap` / `remapObject`, and the masters editor's `GetMaps`) to
  confirm the behaviour a good implementation must have: to remove, drop the
  master, shift every later master's index down one, and drop references that
  pointed into it; to rename, swap only the header entry and its recorded size,
  leaving references alone. **No Wrye Mash source was copied.** The index
  arithmetic itself is dictated by the TES3 file format - a reference packs its
  master index in the top byte of its `FRMR` word - and our `wraithguard/esp/`
  `remove_master` / `rename_master` are independent implementations over our own
  tes3-derived `Reference` model, sharing the same math our (public-domain)
  `merge_to_master` port already uses for master remapping. So no copyleft
  obligations attach; the credit is one of gratitude and correctness.

## Curated data & tooling

- **[Modding-OpenMW.com](https://modding-openmw.com/) (MOMW)** - the curated mod
  lists, the `umo` installer, the MOMW Configurator, and `plugin-order.yml` (the
  source of truth for which plugins belong to which list). This tool is designed
  specifically to *complement* MOMW lists without ever reordering them.
  Customizations are not supported by the MOMW team.
- **tes3cmd** - © 2016 John Moonsugar. MIT.
  ([github.com/john-moonsugar/tes3cmd](https://github.com/john-moonsugar/tes3cmd/))
  The plugin-maintenance Swiss-army knife distributed with the MOMW Tools Pack. Our
  **tes3cmd** window is a front-end that stages plugins with their masters so
  tes3cmd works correctly on a multi-folder OpenMW VFS; we drive the real
  binary for `clean`, and reimplement master-size resync in-app (tes3cmd's own
  sync corrupts headers on this layout). The safe-cleaning workflow (never
  cleaning the vanilla masters, cleaning masters before dependents) is adapted
  from the community "drag-and-drop" cleaning batch by RMWChaos, Pinkertonius,
  and Spirithawke.

## Documentation referenced (no code copied)

- **[UESP](https://en.uesp.net/) - *Morrowind Mod:Mod File Format*.** CC-BY-SA.
  The community's reference for the TES3 binary layout, and the source of
  `wraithguard/tes3fields/schema.py`: 46 record types, their subrecords, whether
  each is required, its declared width, and the named members inside the struct
  ones. What is taken is **format fact** - a `NPDT` is 12 or 52 bytes; its first
  two are a uint16 Level - which describes Bethesda's file format rather than
  anyone's prose about it, and the generated module carries the attribution.

  The schema is what lets the diff window say *what a field is* rather than only
  what its value was, and what backs the **Format reference** view beside a
  record diff. Nothing is guessed: where the tables are ambiguous the schema
  says so (a field with two documented layouts carries neither), and 56 of the
  parsed layouts are checked against the byte counts the same tables declare -
  all 56 agree, which is how we know the parse is right.

  Also the source for the LAND, PGRD and script-record field documentation used
  when writing the binary decoders, alongside the MIT-licensed implementations
  credited above.

## yampt - Yet Another Morrowind Plugin Tools

**yampt - MIT, Copyright (c) 2016-2026 Rafał Wierzchoś.** Licence text vendored
at `License/yampt/LICENSE`.

Four things are ported from yampt's C++, with its permission by licence:

- `status.py` - the two-axis conflict model, from
  `yampt.core/source/scanner/record_conflict.cpp` and the worst-of roll-up in
  `yampt.editor/source/model/nav_tree_model.cpp`. The names (`conflict_all`,
  `conflict_this`, "identical to master", "override wins") follow yampt, which
  in turn follows TES5Edit/xEdit's "Conflict Status All" / "Conflict Status
  This". The absence sentinel is ours: yampt uses a magic byte string because
  its values are C++ strings, and Python can just use an object.

- `wraithguard/gui/pluginview.py` - the plugin tree's shape, from
  `yampt.editor/source/model/nav_tree_model.hpp`: file node -> record-type
  group -> records, each level taking the worst status of its children on both
  axes. yampt follows xEdit here and so do we.

- `align.py` - matching the entries of a repeated field by content identity
  rather than by position, from
  `yampt.core/source/decoder/content_alignment.cpp`. yampt does this over raw
  subrecords with per-record anchor and key rules; working from tes3conv's
  decoded JSON means the entries arrive structured, so what is taken is the
  principle and the question it answers - what identifies an entry - not the
  subrecord machinery.

- `dialogue.py` - resolving a topic into the order the engine reads it, from
  `yampt.core/source/scanner/dial_info_align.cpp`. Including the rule that
  matters most: a response whose `PNAM` names something not yet seen goes to
  the **end** of the topic.

Verified rather than assumed: the port reproduces the file order of all 4,111
topics and 36,735 responses in Morrowind.esm, Tribunal.esm and Bloodmoon.esm
exactly, from the `prev_id` chain alone.

## merge_to_master - whole-plugin merging (public domain)

**merge_to_master** ([`Greatness7/merge_to_master`](https://github.com/Greatness7/merge_to_master))
- © Greatness7, released as **public domain**: asked about its licence, the
author said to "consider it public domain (do whatever you want)." We take that
at its word and record it here so the grant is not lost.

`wraithguard/merge/` is a **port of it**, function by function:

- the `PluginData` bucketing (objects keyed by identity, cells split into
  interiors/exteriors each holding cell + landscape + path grid, dialogue grouped
  into topics with ordered responses), from `types/plugin.rs` and `types/cells.rs`;
- the merge rules - last-plugin-wins objects, unioned cell references, latest
  landscape/path grid, dialogue responses spliced in linked-list order - from
  `traits/merge_objects.rs`;
- the `INFO` ordering with its position-hint index, from `types/dialogue.rs`
  (including the rule that a response whose `prev_id` is not yet present goes to
  the end);
- master-index and land-texture-index remapping, from `traits/remap_masters.rs`,
  `traits/remap_textures.rs` and the `Header` helpers in `traits/extensions.rs`;
- deleted-object removal with the full reference-cleaning graph (which field of
  which record points at which object type) and ignored-object stripping, from
  `traits/remove_deleted.rs` and `traits/remove_ignored.rs`;
- `merge_plugins` and `merge_load_order` with their options (remove-deleted,
  apply-moved-references, remove-duplicate-references), from `merge_plugins.rs`.

The parallel `par_merge_load_order` is deliberately **not** ported - our build is
single-process by design. Verified rather than assumed: our merge reproduces the
original tool's own `Expect.esm` fixtures object-for-object across its dialogue-
ordering, deletion and cell-rename cases (`tests/test_merge_golden.py`), and its
`get_index_remap` unit cases pass unchanged.

## Runtime & optional libraries

- **Python** and **Tkinter/ttk** - the language and GUI toolkit.
- **[tkinterdnd2](https://github.com/pmgagne/tkinterdnd2)** - optional drag-and-drop.
- **[PyYAML](https://pyyaml.org/)** - optional, faster `plugin-order.yml` parsing.
- **[Tauri](https://tauri.app/)** (MIT / Apache-2.0) and `tauri-plugin-dialog` -
  the native window (`viewer-shell/`) behind the cell viewer, the mesh viewer and
  the in-app cell map, on the OS webview (WebView2, WebKitGTK, WKWebView). It
  replaced pywebview.
- **[tkinterweb](https://github.com/Andereoo/TkinterWeb)** /
  **[tkhtmlview](https://github.com/bauripalash/tkhtmlview)** - optional inline
  HTML rendering fallbacks.

## The Rust side

The `wraithguard_native` module (`native/`) and the viewer are built on
greatness7's `tes3` crates (above) and these libraries, all MIT and/or
Apache-2.0 (ryu: Apache-2.0 or BSL-1.0), plus their own dependencies:

- **[PyO3](https://pyo3.rs/)** and **[maturin](https://www.maturin.rs/)** - the
  Python binding and the build that packages it as a wheel.
- **[pythonize](https://github.com/davidhewitt/pythonize)** - serde values
  straight into Python objects, so records reach Python without JSON text in
  between.
- **[serde](https://serde.rs/)** / **serde_json**, **[ryu](https://github.com/dtolnay/ryu)**
  (the shortest float spelling, as tes3conv's JSON writes it), **regex**,
  **base64**, **memmap2**.

## The cell viewer page (GPL-2.0)

`viewer-shell/ui` is licensed **GPL-2.0** and kept apart from the MIT code: it is
the viewer's frontend, loaded into the webview at runtime, not linked into
anything else. Its licence file, `viewer-shell/ui/LICENSE`, carries the full
GPL-2.0 text and the provenance of every part; this is the same list.

**MGE XE** - GPL-2.0, licence text at `License/MGE-XE/LICENSE`. "MGE was written by
Timeslip, LizTail, Krzymar, and Phal. MGE XE is currently being developed by
Hrnchamd" (its readme's credits); "G7" is Greatness7's fork. Ported, with the
credit each shader's own header gives:

- `XE Common.fx` - fog colour and scattering (`06_gl.js`, fed by `25_sky.js`).
- `XE FixedFuncEmu.fx` - the static tonemap and light attenuation (`06_gl.js`).
- `XE Mod Shadow.fx` + `XE Mod Shadow Data.fx` (G7 fork) - the sun shadow
  receiver (`06_gl.js`, `37_shadow.js`).
- `XE Mod Water.fx` (G7 branch, 0.16.0) - the water (`26_water.js`).
- `XE Mod Caustics.fx` - outdoor water caustics (`35_underwater.js`).
- `XE Mod Grass.fx` - groundcover wind (`06_gl.js`).
- `Sunshafts.fx` - "Sun shaft rays by **phal** v0.02a; many tweaks by **Hrnchamd**
  for MGE XE 0.11.0" (`26_water.js`).
- `Underwater Effects.fx` and `Underwater Interior Effects.fx` - **Hrnchamd**
  (`35_underwater.js`).
- `SSAO HQ.fx` - "based on ssao v09 by **Knu**" (`29_ssao.js`).
- `Depth of Field.fx` - "v12 by **Knu**, tweaked by **peachykeen**" (`36_dof.js`).
- `FXAA.fx` - FXAA 3.11 by **Timothy Lottes, NVIDIA**, under NVIDIA's notice in the
  original header; the MGE XE port by **J. Böttcher** (as its header says), posted
  for MGE XE by **Hrnchamd** (`33_fxaa.js`).
- `SSGI.fx` - an MGE XE post shader (HBAO + SSGI): **vtastek**'s SSGI, modified
  by **Remiros**; GPL-2.0 like the other MGE XE shaders (the SSGI mode in
  `29_ssao.js`).

**Gardenfell** (Robin Hjelte, MIT; see above) - the page itself, including Robin's
camera smoothing from his own MWSE mod, and his go-ahead for the SSAO port.

**Ours** (StaticNation, GPL-2.0 as part of the page) - `23_viewer_profile.js`,
`24_ori.js`, `38_wg_viewport.js`, `39_wg_coverage.js`, `40_wg_modhl.js`,
`41_wg_meshview.js` and `viewer_only.html`.

The water's other sources - the shore surf (mod 56186), the sewer waves (mod
45432) and the webgl-noise simplex noise - are in the three sections that follow.

## Cell viewer water: wave randomness

The cell viewer's water is MGE XE's water shader, as ported in Gardenfell's viewer (see Gardenfell above and `License/MGE-XE`). To keep its
waves from repeating tile by tile, a slow **2D simplex noise** field bends where the
wave texture is read, puts stretches of water out of step with each other, and varies
the wave strength a little (`viewer-shell/ui/src/26_water.js`, `waterNormal`). The
`snoise`/`wgPermute`/`wgMod289` functions are **Ian McEwan / Ashima Arts** and
**Stefan Gustavson**'s implementation from the `webgl-noise` project, MIT licensed,
reproduced as the licence permits (`License/webgl-noise/LICENSE`). The technique, a
noise-driven domain warp, is the one the retired three.js water used.

## Water shader with foam on shore (Nexus Mods, Morrowind mod 56186)

The cell viewer's water carries this mod's shore foam (the surf) and procedural
caustics in the shallows (`viewer-shell/ui/src/26_water.js`), ported from its
`water.frag` - the author's own foam and caustics functions only, not the OpenMW
water shader they sit in. Used under the mod's
permission: "Feel free to use this shader as a resource for your own projects but
for the fog part you must ask Epoch." The fog is not used.

## Enhanced Water Shader for MGE XE 2.0 Green-Blue (Nexus Mods, Morrowind mod 45432)

The cell viewer's sewer waves - the rings spreading from the sewer outlets of Vivec
and Molag Mar, added to MGE XE's close wave normals - are this shader's, ported to
GLSL in `viewer-shell/ui/src/26_water.js` (`sewerWaves`): its outlet positions and
ring formula. Credits as its readme gives them: its author; vtastek (sewer wave
optimisations); phal and harnlarnm (sewer waves); abot (sewer waves port); built on
MGE XE's water shader. Permission: "You can do with this shader what you want as
long as you give proper credit to the original authors and me."

## And of course

- **Bethesda Game Studios** - for *The Elder Scrolls III: Morrowind*.
- The wider **OpenMW and Morrowind modding community** - for decades of tools,
  documentation, and reverse-engineering that everything here depends on.

---

*Wraithguard Toolkit is provided as-is. Where we reproduce MIT-licensed material
(notably tes3lint's evil-GMST table, and the algorithms ported from Merged
Lands and yampt), the original copyright and licence notice travels with it in
the source. GPL source appears only in the cell viewer's page (`viewer-shell/ui`),
which is itself GPL-2.0, carries the full licence text, and is kept apart from the
MIT code; MWSE and OpenMW were read for cross-checking only, and the unlicensed
community Perl scripts contributed ideas, not code.*

*Attribution is something we would rather over-do than get wrong. If anything
here is inaccurate - a name, a licence, a claim about what we derived from
whom - please tell us and it will be corrected.*
