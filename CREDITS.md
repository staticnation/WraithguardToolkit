# Credits & Acknowledgements

Wraithguard Toolkit stands on the work of a lot of other people. This tool exists
because these projects were generous enough to share their code, formats, and
research. Huge thanks to everyone below - the good ideas are theirs; any bugs are
ours.

If you are one of these authors and want your attribution changed (or removed),
please get in touch and we'll fix it right away.

Since 5.0 the toolkit is one Rust program: `wraithguard-toolkit` (Tauri, in
`viewer-shell/`) and the `wraithguard` command line (`viewer-shell/cli`). The 4.x
Python/Tk app is retired. Where an entry below says something was ported, the port now
lives in the crate named; several were first ported to Python and then carried to Rust
step for step, pinned by the cases in `tests/fixtures/` that the Python answered.

---

## Code ported or adapted from (MIT-licensed)

These projects are MIT-licensed. We ported logic and/or cross-referenced their
implementations; their copyright notices are reproduced with the relevant parts,
and the license files of those whose code or data the program carries are in `License/`
(shipped in every package). Projects only read for reference have none there.

- **mlox** - © 2009–2017 John Moonsugar (alias), dragon32, Arthur Moore. MIT.
  The load-order rule engine and the rule databases (`mlox_base.txt` /
  `mlox_user.txt`) this whole tool is built around. Our matching, ordering, and
  `[Conflict]/[Requires]/[Note]` predicate logic is a port of mlox's
  (`viewer-shell/ordercore`: `rules.rs`, `predicates.rs`, `patterns.rs`, `order.rs`).
  Several of our **Lint** checks (evil GMSTs, the interior fog-density-0 bug,
  expansion-function dependencies) come from mlox's `tes3lint`, credited
  separately below. (The missing-pathgrid check does *not* come from mlox -
  see abot's scripts below.)
- **tes3lint** - © 2009 John Moonsugar. MIT. Distributed as part of mlox.
  A diagnostic tool for TES3 plugins. Our **Lint** tool
  (`viewer-shell/toolscore/src/lint.rs`) reimplements its useful checks against plugin
  binaries, so they see the whole OpenMW multi-folder VFS, with no Perl needed. One thing
  is *reproduced rather than reimplemented*: the table of **72 "evil GMSTs"** - the exact
  name/value pairs an old Construction Set wrote when run without both expansions. Those
  values are research, not something we could rederive, and the table carries John
  Moonsugar's copyright notice inline (`EVIL_GMSTS` in `toolscore/src/lint_tables.rs`).
- **[mlox-rules](https://github.com/DanaePlays/mlox-rules)** - maintained by
  DanaePlays and contributors. The **actively-updated** rule database that
  modern mlox (v1.1+) and plox both use. The Load order page's **Update rules**
  downloads the current `mlox_base.txt`/`mlox_user.txt` from this repo.
- **plox** - © 2024 Moritz Baron. MIT.
  A Rust reimplementation of mlox. Used as a second reference to harden our
  engine (wildcard/`<VER>` matching, order transitivity, predicate functions).
- **tes3conv** - © 2025 Greatness7. MIT.
  Converts Morrowind plugins ↔ JSON. It was the record-identification and
  field-diff engine behind the 4.x Check Conflicts, and the MOMW Tools Pack shipped it for
  this tool. It is no longer run: the `tes3` crate (below) does the same work in process.
  Its JSON layout lives on: the records the toolkit works with still take tes3conv's
  shape, field names and all.
- **tes3** ([`Greatness7/tes3`](https://github.com/Greatness7/tes3)) - © Greatness7. MIT.
  The Rust library of TES3 record and NIF block types, and the toolkit's foundation:
  every crate reads and writes plugins on `tes3::esp`, reads meshes on `tes3::nif` (the
  mesh viewer's block panel and field editor, the Resource Conflicts summary) and reads
  `.bsa` archives on `tes3::bsa`. The cell viewer's engine (`viewer-shell/viewcore`) reads
  through it too (`viewer-shell/gardenfell-g7`), and falls back to Gardenfell's own
  readers only for a file or record the crate refuses. The build takes it from our fork,
  [`staticnation/tes3`](https://github.com/staticnation/tes3), until the changes waiting
  upstream land.
- **R-Zero's Construction Tools** and its **Door marker and Travel marker replacer**
  and **Leveled Creature marker direction fix** - R-Zero aka Reizeron (Nexus Mods,
  Morrowind mod 47908, May Modathon 2020; inspired by LondonRook's Outlander mod
  markers). The author's permission: "Do whatever you want, really." Five of its
  meshes are built into the cell viewer (`viewer-shell/viewcore/data/markers`, see its
  README): door and travel markers, the leveled-creature marker, the character gauge
  and the ruler.
- **Merged Lands** - © 2022 David Von Derau. MIT. License text vendored at
  `License/MergedLands/LICENSE`.
  A Rust tool that merges the landscape changes a load order would otherwise
  discard. `viewer-shell/landcore` is a **port of it**, function by function - the
  reference landmass, the per-mod diff, the four conflict strategies and their
  thresholds, corner-then-edge seam repair, cleaning, the shared `LTEX` index space, the
  conflict images, and the `.mergedlands.toml` sidecar schema, which we read with the
  same field names and values so settings written for the original work here unchanged.
  It was first ported to Python and carried to Rust from there, with our fixes kept,
  pinned to the Python's answers byte for byte (`tests/fixtures/land_merge_cases.json`).

  `MERGED_LANDS.md` accounts for **all 191 functions** in its `src/`, names where each
  one lives here, and records every place we diverge and why. Our additions - a slope
  limiter, curvature-weighted resolution, and digest-based cleaning - are marked as ours
  there rather than attributed to it.

- **Merged Lands, OpenMW fork** (`OpenMWMergedLands`) - MIT, retaining © 2022
  David Von Derau; license identical to the original's, `License/MergedLands/LICENSE`.
  The OpenMW-oriented continuation of Merged Lands (reads `openmw.cfg`, emits
  `.omwaddon`, app-config `merged_lands.toml`). We compared our port against it
  file by file - the *fork comparison* section of `MERGED_LANDS.md` records the
  result: it is a modernization and OpenMW-integration pass, not an
  algorithm-fix pass. From it we adopted the default texture-fallback behavior
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
    textures read for drawing, the sky and weather. Since the 18hu rebase it is a
    layer over seven of his engine crates, vendored unmodified in
    `viewer-shell/gardenfell` (his `LICENSE` and `NOTICE`; `VENDOR.md` lists them,
    with the patches we carry and offer back). `viewer-shell/gardenfell-g7` (ours,
    MIT) has greatness7's `tes3` crate read for them.
  - **The command layer**, `viewer-shell/src/cellviewer/commands.rs` (MIT): what
    the page calls.
  - **The page**, `viewer-shell/ui` (**GPL-2.0**): the WebGL2 renderer, terrain,
    water, sky, particles, shadows, ambient occlusion, depth of field, underwater,
    the cell picker and its map. The MGE XE shader ports (shadows, fog/tonemap,
    water, sunshafts, underwater, SSAO) are why this part is GPL. It is a separate
    frontend loaded into the webview, so the GPL does not reach the MIT code. See
    `viewer-shell/ui/LICENSE`, `License/Gardenfell` and `License/MGE-XE`.

  Rebased on Robin's 18hu page in October 2026 (`viewer-shell/GARDENFELL.md`); his
  parts now carry his license line at their top, and the MGE XE ports sit in his
  `06_mge.js`. What we changed: stripped it to a viewer (grass generation, painting, the
  statics setup, ESP export and the profile/rules managers are gone from the page,
  the commands and the engine); it opens Wraithguard's own setup; the engine reads with
  greatness7's `tes3` crates and keeps Robin's readers as the fallback (his pill bar, side
  panel sizing, find bar and window memory came in with it, the window placement in
  `cellviewer/mod.rs` too); and ours are the ORI inspector, `groundcover=` loading, the
  mod-coverage map in the cell picker, mod highlighting, the viewport settings, the
  mesh-viewer mode with its block editor, the Load order, Conflicts, Editor, Land and Tools
  pages, and the water changes credited at the end of this file.
  `viewer-shell/MANIFEST.md` lists every part.

  Two smaller things came from it earlier:
  - Its tiny hand-crafted animation test NIFs (`morph.nif`, `anim.nif`,
    `uvsets.nif`, `particle_move.nif`, `flap.nif`/`.kf`) are vendored under
    `tests/fixtures/gardenfell_anim/` (MIT). They first tested our own NIF
    reader's animation extraction, where they caught two bugs -- geometry lost
    under a `NiCollisionSwitch`, and a particle `NiTriShape` drawn as a surface --
    and now test the crate-based mesh readers in `viewer-shell/conflictcore`.
  - Its start-up discipline -- *read a thing once and reuse it rather than
    re-reading it every pass* -- shaped our first cell previewer, before that
    previewer was replaced by Gardenfell's own.

- **meshoptimizer** ([`zeux/meshoptimizer`](https://github.com/zeux/meshoptimizer)) -
  © 2016-2026 Arseny Kapoulkine. MIT. License text at `License/meshoptimizer`.
  `viewer-shell/meshopt-port` is a line-for-line Rust port of version 1.3's vertex fetch
  order and remaps, its vertex fetch, overdraw and coverage analyzers, and its whole
  simplifier (`simplify`, with attributes, with update, sloppy, prune, points, scale),
  beside Gardenfell's own port of its vertex cache and overdraw passes (`triorder`, Robin
  Hjelte); the two together are the mesh viewer's draw-order optimizer
  (`viewer-shell/conflictcore/src/nif_order.rs`), the same pipeline as greatness7's
  Blender exporter. No C++ is built or shipped; the C++ is only run, from its own
  repository, to make the values the port is tested against (`tools/meshopt_oracle`).

- **momw-configurator** - © Modding-OpenMW.com (johnnyhostile). MIT.
  We read its `cfg/custom.go` to reimplement its customization-apply logic
  faithfully (`viewer-shell/ordercore`: `apply.rs`, `export.rs`), so the **Export
  preview** can simulate exactly what the Configurator will do to your `openmw.cfg`
  (matching, insert/replace/remove order, ambiguity errors) before it runs.
- **modmapper** - © 2023 Michiel. MIT.
  The inspiration and reference for the cell-map heatmap (which mods touch which
  exterior/interior cells).
- **Tes3EditX** - © 2023 Moritz Baron. MIT.
  Referenced for TES3 record handling and conflict-resolution UX.
- **TES3Tool** - © 2019 SaintBahamut. MIT.
  Referenced for the TES3 binary record/subrecord layout.
- **MWEdit** - © Dave Humphrey and contributors. MIT.
  Its `data/Functions.dat` and `mwedit/script_defs.h` are the primary source
  for our script-bytecode opcode table (`viewer-shell/editcore/data/opcodes.json`): the
  function names, their opcode values, and the parameter-flag words that describe each
  operand's encoding. Without it the script disassembly would be guesswork - and
  guesswork is exactly what we refused to ship.

  The compiler-internal opcodes that no function table lists (notably
  `_SetReference`, emitted for `id->Func`) were **measured from a corpus of
  real compiled scripts** rather than taken from anyone's source - an opcode's
  numeric value is a fact about the game's own data files.

  The **script compiler** (`viewer-shell/editcore/src/mwscript/`) is a port of MWEdit's
  (`mwedit/script_compile.cc`, `script_compile_ex.cc`; © 2025 Walrus Tech, originally
  Dave Humphrey; MIT). Its parse tables, function table, magic effect IDs and animation
  groups are generated from MWEdit's source (`mwscript/data.rs`), and MWEdit's compiler
  test plugin (`data/MwEditScriptTest.esp`, with its license) is in
  `tests/fixtures/mwedit/` as the reference it is checked against byte for byte.
  Its `data/customfunctions.dat` (the MWSE / MW-Enhanced function list MWEdit ships) is
  read as the compiler's default set of extended functions.
- **Morrowind Dialog Explorer (MWDE)** - © 2018 Sophie Kirschner
  ([pineapplemachine.com/files/mwde](https://pineapplemachine.com/files/mwde)).
  MIT. The Editor's dialogue views (`viewer-shell/editcore/src/dialogue_graph.rs`,
  `journal.rs`) turn a DIAL/INFO record into the line an NPC says, to whom, and under
  what conditions. The condition phrasing - the mapping from a SCVR filter's
  type/function/comparison to an English "If ..." line, the boolean special
  cases ("is not dead", "is a member of faction X"), the speaker-context
  assembly - and the result-script token lexer (from MWDE's `src/syntax_highlight.py`)
  are adapted from MWDE's `src/info_string.py`, retargeted from its raw-subrecord model
  onto the enum names the `tes3` crate uses. The **function labels themselves** follow
  the `tes3` crate's own `FilterFunction` names (prettified) rather than MWDE's table.
  MWDE's wider record/field schema was reviewed but not ported.

## abot's tes3cmd scripts (idea credited, no code used)

- **`missing_pathgrids.pl`** and **`cell_conflicts.pl`** - © **abot**.
  Published as *Missing Pathgrids* and *Cell Conflicts* on abot's own site,
  ["Morrowind is Home"](https://abitoftaste.modlist.x10.mx/morrowind/index.php?option=downloads&catid=58&Itemid=50&-Morrowind-tools)
  (Downloads → Morrowind tools), alongside MMOG, MRS and abot's other tools.

abot is behind a great deal of what makes Morrowind still worth playing -
Water Life, Silt Striders, the merged-object and resource-scanning tools that
half the community's load orders depend on. These two `tes3cmd --program-file`
scripts are small by comparison and easy to overlook, so: thank you.

**The files themselves carry no copyright line and no license text.** No
license granted means the author keeps all rights, which makes these the most
restricted inputs in this project rather than the least. So we treated them as
*read-only inspiration*: **no line of either script is in this tool**, and both
of our implementations were written from scratch against plugin binaries. What we
took is the diagnostic idea, which copyright does not cover.

- *Missing Pathgrids* - the idea: an interior cell with no `PGRD` record is a
  bug, because NPCs cannot pathfind there. Our `[NO PATHGRID]` Lint check
  (`toolscore/src/lint.rs`) deliberately *diverges*: the original only considers
  plugins earlier in the load order and so reports false positives, whereas ours
  accepts a pathgrid contributed by **any** plugin.
- *Cell Conflicts* - the idea: "show me every mod touching the same cells as
  this one." That became the **Focus on mod** filter in our cell map (whose
  implementation follows *modmapper*, MIT).

abot - if you would rather we credit this differently, drop the mention, or not
reference your scripts at all, say the word and it is done.

## Approach referenced (no code copied)

- **MGE XE** (Greatness7's Rust rewrite, `MGE-XE-main`) - **GPL-2; no code copied.** The
  Cell Preview's distant objects (`viewer-shell/viewcore/src/lod.rs`, `distant_objects`)
  budget as its distant statics do: near/far/very-far size tiers and their default
  thresholds and bands, and its static simplifier's settings (error-driven, welded,
  normals/UVs/colors as weighted attributes, borders locked).

- **Zhao, Jiang & Guo**, "A novel quadratic error metric mesh simplification algorithm
  for 3D building models based on 'local-vertex' texture features" (ISPRS Archives
  XLVIII-3/W2-2022, Urban Geoinformatics 2022). The Cell Preview's texture-aware
  simplify mode for distant objects (`viewer-shell/viewcore/src/lod.rs`) follows its
  idea - keep more where the texture under the surface is busy and where it bends -
  through meshoptimizer's own per-vertex priority, not the paper's own cost function.

- **TES3 Conflictsolver Editor** - © 2026 kirgan
  (a Mini-TES3Edit–style patch tool). No license file is
  distributed with it; **no code was copied**. We credit it for the field-level
  record-diff *approach* behind our field comparison (`viewer-shell/conflictcore/src/fields.rs`:
  a record flattened to dotted keys, objects opened and lists kept whole). All rights
  remain with its author.
- **The Elder Scrolls Construction Set** (Bethesda) and **Construction Set Extender**
  (CSSE, the MWSE team) - **no code copied**. The Editor follows the CS's
  window model as its help documents describe it (the Object Window's record tabs and
  sortable columns, the Cell View's cells and their references, the render window, an
  edit dialog per record; the help text read from the Morrowind Modding community's
  Markdown edition, morrowind-modding/tes3cs-manual, https://tes3cs.pages.dev/), and
  takes CSSE's additions as its checklist (an Object Window filter on Ctrl+F, changed
  records marked green, F2 to edit the selected reference's base record; from MWSE's
  `docs/source/references/general/csse.md`). Window layout and behavior only; every
  line of the Editor is ours.
- **xEdit / TES5Edit / SSEEdit** - © the xEdit team. **MPL 1.1; no code copied.**
  Our conflict-color convention -- a record's overall status colors the row
  **background**, what one plugin does colors the **text** -- is xEdit's, the
  one every modder already reads (green = no conflict, yellow = benign override,
  red = a losing edit; see the STEP guide's color legend). The specific hues on the
  Conflicts page (`viewer-shell/ui/src/60_wg_conflicts.js`) are the Material-Design values
  from a community *Material dark-mode edit of xEdit's `xEdit.ini`* (its
  `[ColorConflictThis]`/`[ColorConflictAll]` sections, Delphi `TColor` integers
  we decoded to `#RRGGBB`). Only the color **values and the scheme** were
  taken -- data, not code. Material Design is Google's, under Apache-2.0.

## Meshes, archives and textures: what was ours, and Greatness7's relicense

Until 4.2.0 the toolkit read meshes, archives and textures with its own code, written
from the public format descriptions and checked against real files, because every
ready-made reader was either copyleft (pyFFI, nifly, NifSkope, the GPL-3.0 `nif.xml`,
`pydds`) or too heavy for what Morrowind needs (bethesda-structs). That reader is
retired: meshes and archives are read by greatness7's `tes3` crate and Gardenfell's
readers (above). Its full provenance - every field layout and how it was derived - is in
the repository's history (`NIF_PROVENANCE.md`, retired in 5.0.1).

### Greatness7 relicensed the `es3` library

On 28 July 2026, **Greatness7** - author of the Morrowind Blender Plugin and of
`Greatness7/tes3` - offered to relicense the NIF library inside `io_scene_mw`,
and then did it:
[`cbe18b5`](https://github.com/Greatness7/io_scene_mw/commit/cbe18b558299e14ecd959183e3cf9ea096fe95df)
adds an MIT `LICENSE` to `lib/es3/`. `Greatness7/tes3` was already MIT. Nothing of
`es3` is in the program now (it was the old reader's cross-check), so its license is no
longer carried.

That was an unprompted act of generosity toward a project that had spent months
carefully working around his code, and it is worth naming plainly. The
relicensed library is `lib/es3/` **only**; the rest of `io_scene_mw` remains
GPL-3.0 because Blender requires plugins to be, and this project respects that
line. Cross-checked against it, our reader found a gap it could not have found
alone: `NiUnionBV`, a bound type no file in either corpus carries. A second
implementation sees what a corpus cannot.

### Textures

The texture decoders are ours (`viewer-shell/viewcore/src/bcx.rs` for BC4, BC5 and BC7;
`img.rs` hands BC1-BC3 to the GPU still compressed). The BC7 tables come from the
**published format specification** - Khronos's OpenGL BPTC specification and
Microsoft's Direct3D 11 documentation - not from any implementation. When they were
written, Pillow was the **oracle**: the corpus textures decoded byte for byte identically
to it, and BC7 matched on 19,380 random blocks across every mode and partition. Used,
never read. **`pydds` was rejected on license** (GPLv3-or-later, which would relicense
the whole project); `quicktex` (Apache-2.0) was never needed.

## mermaid.js - bundled, for the script charts

`viewer-shell/ui/assets/mermaid.min.js` is **mermaid 12.1.0** (© 2014 - 2022 Knut
Sveidqvist and contributors), MIT licensed, with its license text beside it as
`mermaid-LICENSE.txt` (and in `License/Mermaid`). It is upstream's own
`dist/mermaid.min.js` from the npm package, unmodified (SHA-256
`6484afc32872a3aa16cac9a76ba1816a1ed4cc870a6593cc2e17757750f518b2`). The Lua and MWScript
flowcharts and call graphs draw with it (`viewer-shell/luacore/src/mermaid.rs`,
`viewer-shell/editcore/src/mwscript/flowchart.rs`).

## Teal and htl - the Lua checks (MIT)

`viewer-shell/luacore` embeds **htl** (ynishi/htl, MIT OR Apache-2.0), which carries the
**Teal** compiler `tl.lua` (Hisham Muhammad and contributors, MIT) and runs it in a
vendored Lua 5.4 through **mlua** (MIT). They check OpenMW Lua and Teal scripts
(`luacore/src/check.rs`, `teal.rs`).

**OpenMW's Lua API documentation (GPLv3) is read, never shipped.** OpenMW documents its
Lua API in `files/lua_api/openmw/*.lua` and installs those files to every install's
`resources/lua_api/`; the built-in interfaces are documented in `resources/vfs/`. They
are GPLv3. The toolkit contains none of them: `viewer-shell/luacore/src/ldt.rs` reads them
from the user's own OpenMW install at run time and writes Teal declarations into a
temporary folder, as OpenMW's own `openmwluadocumentor` does for its published
`teal_declarations`. The reader was written from the comment format (LDT's `@module`,
`@type`, `@field`, `@function`, `@param`, `@return`), and its tests use fixtures written
for them. The same separation as the cell viewer below: the GPL part stays where its
license puts it, and the toolkit stays MIT.

**OpenMW's LuaJIT and built-in scripts are run from the install, never shipped.** The
Preview's Scripts panel runs OpenMW Lua (`viewer-shell/viewcore/src/luarun.rs`) on the
user's own OpenMW build: its LuaJIT (MIT; `lua51.dll`, `lib/libluajit-5.1.so.2` or
`Frameworks/libluajit-5.1.*.dylib`) loaded at run time with **libloading** (Simonas
Kazlauskas, ISC), and its `resources/vfs` and `vfs-mw` built-in scripts (GPLv3) read from
the install as OpenMW reads them. Where an install has no LuaJIT, the viewer's own Lua 5.4
(mlua, above) runs them. The API they run against (`luarun.lua`) is our own, written from
OpenMW's documentation.

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
  (`components/nifosg/controller.cpp`) -- and for the sound volumes and ranges
  (`soundbuffer.cpp`), which are format behavior, not code: our implementations are
  independent.
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
  MWEdit-derived entry is kept**. 25 of the 26 are the same call: MWEdit says `String`
  where customfunctions says `Long | String` for a filename or object id, and UESP's
  per-function pages document those parameters as strings, which settles it.
  The 26th is a genuine error and is corrected: MWEdit gives
  `XFileWriteFloat` a single float operand with no filename, where
  customfunctions lists two and UESP documents
  `xFileWriteFloat filename (string), value (float)`.
- **MGE XE** - GPL-2.0. Read alongside MWSE for the same opcode cross-check; no
  MGE XE source is in the MIT code. Its **shaders are ported into the cell viewer's
  page**, which is GPL-2.0 for exactly that reason - see *The cell viewer page
  (GPL-2.0)* below.
- **Wrye Mash** (Polemos fork) - © 2017-2021 Polemos, based on Yacoby
  (2011-2016), Melchor (2009-2011) and Wrye (2005-2009). **GPLv2-or-later.**
  Wrye Mash popularized the *features* of removing a master from a plugin and
  re-pointing one at a renamed file ("Change to.."), and we read its `mosh.py`
  (`FileRefs.remap` / `remapObject`, and the masters editor's `GetMaps`) to
  confirm the behavior a good implementation must have: to remove, drop the
  master, shift every later master's index down one, and drop references that
  pointed into it; to rename, swap only the header entry and its recorded size,
  leaving references alone. **No Wrye Mash source was copied.** The index
  arithmetic itself is dictated by the TES3 file format - a reference packs its
  master index in the top byte of its `FRMR` word - and our
  `viewer-shell/toolscore/src/masters.rs` (`remove_master`, `rename_master`) is an
  independent implementation on the `tes3` crate. So no copyleft obligations attach;
  the credit is one of gratitude and correctness.

## Curated data & tooling

- **[Modding-OpenMW.com](https://modding-openmw.com/) (MOMW)** - the curated mod
  lists, the `umo` installer, the MOMW Configurator, and `plugin-order.yml` (the
  source of truth for which plugins belong to which list). This tool is designed
  specifically to *complement* MOMW lists without ever reordering them.
  Customizations are not supported by the MOMW team.
- **tes3cmd** - © 2016 John Moonsugar. MIT.
  ([github.com/john-moonsugar/tes3cmd](https://github.com/john-moonsugar/tes3cmd/))
  The plugin-maintenance Swiss-army knife distributed with the MOMW Tools Pack. Our
  **tes3cmd** tool (`viewer-shell/toolscore/src/tes3cmd.rs`) is a front-end that stages
  plugins with their masters so tes3cmd works correctly on a multi-folder OpenMW VFS; we
  drive the real binary for `clean`, and reimplement master-size resync in-app (tes3cmd's
  own sync corrupts headers on this layout). The safe-cleaning workflow (never
  cleaning the vanilla masters, cleaning masters before dependents) is adapted
  from the community "drag-and-drop" cleaning batch by RMWChaos, Pinkertonius,
  and Spirithawke.

## Documentation referenced (no code copied)

- **[UESP](https://en.uesp.net/) - *Morrowind Mod:Mod File Format*.** CC-BY-SA.
  The community's reference for the TES3 binary layout. What is taken is **format
  fact** - a `NPDT` is 12 or 52 bytes; its first two are a uint16 Level - which
  describes Bethesda's file format rather than anyone's prose about it. It was the
  source of the 4.x field schema (46 record types, their subrecords and struct members),
  and is still the reference for the LAND, PGRD and script-record layouts the binary
  readers follow, alongside the MIT-licensed implementations credited above.

## yampt - Yet Another Morrowind Plugin Tools

**yampt - MIT, Copyright (c) 2016-2026 Rafał Wierzchoś.** License text vendored
at `License/yampt/LICENSE`.

Three things are ported from yampt's C++, with its permission by license:

- The two-axis conflict model (`viewer-shell/conflictcore/src/fields.rs`), from
  `yampt.core/source/scanner/record_conflict.cpp` and the worst-of roll-up in
  `yampt.editor/source/model/nav_tree_model.cpp`. The names (`conflict_all`,
  `conflict_this`, "identical to master", "override wins") follow yampt, which
  in turn follows TES5Edit/xEdit's "Conflict Status All" / "Conflict Status
  This".

- The Conflicts page's plugin view, from
  `yampt.editor/source/model/nav_tree_model.hpp`: file node -> record-type
  group -> records, each level taking the worst status of its children on both
  axes. yampt follows xEdit here and so do we.

- Resolving a topic into the order the engine reads it
  (`viewer-shell/editcore/src/dialogue.rs`), from
  `yampt.core/source/scanner/dial_info_align.cpp`. Including the rule that
  matters most: a response whose `PNAM` names something not yet seen goes to
  the **end** of the topic.

Verified rather than assumed: the port reproduces the file order of all 4,111
topics and 36,735 responses in Morrowind.esm, Tribunal.esm and Bloodmoon.esm
exactly, from the `prev_id` chain alone.

(The 4.x app also ported yampt's content alignment of repeated fields,
`content_alignment.cpp`; that view was not carried into 5.0.)

## merge_to_master - whole-plugin merging (public domain)

**merge_to_master** ([`Greatness7/merge_to_master`](https://github.com/Greatness7/merge_to_master))
- © Greatness7, released as **public domain**: asked about its license, the
author said to "consider it public domain (do whatever you want)." We take that
at its word and record it here so the grant is not lost.

The Tools page's **Merge to master** (`viewer-shell/toolscore/src/mergemaster.rs`) is
**greatness7's own code**, called as a library: a plugin folded into its master, or a
whole load order folded into one master, as the game would resolve it. The build takes
the crate from our fork
([`staticnation/merge_to_master`](https://github.com/staticnation/merge_to_master)),
with greatness7's `tes3` from ours too, until the changes still waiting upstream land
there (upstream already took our stable-Rust change, commit 5ea27f1).

The Editor's patch writer (`viewer-shell/editcore/src/build.rs`) takes the crate too: an
OpenMW Lua script list carried into a patch has the references in its Lua data
renumbered by the crate's `remap_refnums`.

Before it was used as a library, the 4.x app carried a function-by-function Python port
of it, which reproduced the original tool's own `Expect.esm` fixtures; that port is
retired.

## The program's libraries

The program and its crates are built on greatness7's `tes3` crates and Robin Hjelte's
Gardenfell crates (above) and these libraries, all MIT and/or Apache-2.0 unless noted,
plus their own dependencies. Each is used unmodified, from crates.io.

- **[Tauri](https://tauri.app/)** (MIT / Apache-2.0) and `tauri-plugin-dialog` - the
  native window around the page, on the OS webview (WebView2, WebKitGTK, WKWebView), and
  the AppImage bundler.
- **[serde](https://serde.rs/)** / **serde_json** (with **[ryu](https://github.com/dtolnay/ryu)**,
  Apache-2.0 or BSL-1.0, the shortest float spelling, as tes3conv's JSON writes it),
  **[toml](https://github.com/toml-rs/toml)** and
  **[serde-saphyr](https://github.com/bourumir-wyngs/serde-saphyr)** (the
  `.mergedlands.toml` and `momw-customizations.toml`; `plugin-order.yml`).
- **[ureq](https://github.com/algesten/ureq)** - downloading rule files and
  `plugin-order.yml`.
- **regex**, **base64**, **memmap2**, **indexmap**, **sha2**, **anyhow**,
  **[encoding_rs](https://github.com/hsivonen/encoding_rs)** (the game's Windows-1252
  text; Apache-2.0 or MIT, its data under BSD-3) and **[zstd](https://github.com/gyscos/zstd-rs)**
  (MIT; the zstd library itself is BSD).
- **[rusqlite](https://github.com/rusqlite/rusqlite)** (MIT), with **SQLite** (public
  domain) bundled - the Editor's record cache, and the viewer reads OpenMW's
  `navmesh.db` through it.
- **[Symphonia](https://github.com/pdeljanov/Symphonia)** (MPL-2.0) - the viewer decodes
  sounds with it (WAV in PCM and ADPCM, MP3, Ogg Vorbis, FLAC; `viewcore/src/audio.rs`) for
  the page to play. MPL-2.0 is per-file copyleft, so Symphonia's own files stay under it
  and nothing of ours does. Its source is at the link.
- **[mlua](https://github.com/mlua-rs/mlua)** and **libloading** (ISC) - OpenMW Lua in
  the Preview (see *Teal and htl* above).
- **[Rapier](https://rapier.rs/)** and **[Parry](https://parry.rs/)** (Dimforge, Apache-2.0),
  with **[glam](https://github.com/bitshifter/glam-rs)** (MIT/Apache-2.0) for their math -
  the Editor's Physics drop (`viewer-shell/physcore`). OpenMW itself uses Bullet; it does
  not simulate loose objects, so the drop is a placing tool and only the collision it
  falls onto is the game's. glam also does the landscape's curvature
  (`landcore/src/curvature.rs`).
- **[clap](https://github.com/clap-rs/clap)** and
  **[chrono](https://github.com/chronotope/chrono)** - the `wraithguard` command line's
  arguments and its clock (`viewer-shell/cli`).

**OpenMW's navmesh.db is read, not OpenMW's code.** OpenMW (GPLv3) writes the file;
`viewer-shell/viewcore/src/navmesh.rs` reads it from the file's layout (the SQLite
schema, the blobs' bytes), with its own LZ4 block decoder written from the LZ4 block
format's public description. Nothing of OpenMW's source is in it. The polygon mesh it
reads is Recast's (Mikko Mononen, zlib), as stored.

## The cell viewer page (GPL-2.0)

`viewer-shell/ui` is licensed **GPL-2.0** and kept apart from the MIT code: it is
the viewer's frontend, loaded into the webview at runtime, not linked into
anything else. Its license file, `viewer-shell/ui/LICENSE`, carries the full
GPL-2.0 text and the provenance of every part; this is the same list.

**MGE XE** - GPL-2.0, license text at `License/MGE-XE/LICENSE`. "MGE was written by
Timeslip, LizTail, Krzymar, and Phal. MGE XE is currently being developed by
Hrnchamd" (its readme's credits); "G7" is Greatness7's fork. Ported, with the
credit each shader's own header gives:

- `XE Common.fx` - fog color and scattering (`06_gl.js`, fed by `25_sky.js`).
- `XE FixedFuncEmu.fx` - the static tonemap and light attenuation (`06_gl.js`).
- `XE Mod Shadow.fx` + `XE Mod Shadow Data.fx` (G7 fork) - the sun shadow
  receiver (`06_gl.js`, `37_shadow.js`).
- `XE Mod Water.fx` (G7 branch, 0.16.0) - the water (`26_water.js`).
- `XE Mod Water.fx`'s dynamic ripples - the displaced `WaterVS` and the radial water
  mesh (`distantinit.cpp`) - and its reflection blur (`26_water.js`).
- The precipitation ripples - `renderwater.cpp`'s drops and `XE Main.fx`'s
  `WaveStepPS` wave simulation (`43_wg_rain.js`).
- `textures/MGE/water_NRM.dds`, MGE XE's wave volume, **shipped unmodified** as
  `viewer-shell/ui/assets/water_NRM.dds` for setups without it: MoMW runs OpenMW, which
  has no MGE XE files, and without it the waves would be a stand-in.
- `XE Mod Caustics.fx` - outdoor water caustics (`35_underwater.js`).
- `XE Mod Grass.fx` - groundcover wind (`06_gl.js`).
- `Sunshafts.fx` - "Sun shaft rays by **phal** v0.02a; many tweaks by **Hrnchamd**
  for MGE XE 0.11.0" (`26_water.js`).
- `Underwater Effects.fx` and `Underwater Interior Effects.fx` - **Hrnchamd**
  (`35_underwater.js`).
- `SSAO HQ.fx` - "based on ssao v09 by **Knu**" (`29_ssao.js`).
- `Bloom Fine.fx` - **Hrnchamd** (`42_wg_bloom.js`).
- `XE Mod Sky.fx` - the ordered dither over the sky's gradient (`06_gl.js`).
- `Depth of Field.fx` - "v12 by **Knu**, tweaked by **peachykeen**" (`36_dof.js`).
- `FXAA.fx` - FXAA 3.11 by **Timothy Lottes, NVIDIA**, under NVIDIA's BSD-3 license
  (`License/FXAA/LICENSE`, the FXAA 3.11 header from NVIDIA's GameWorks samples); the
  MGE XE port by **J. Böttcher** (as its header says), posted for MGE XE by **Hrnchamd**
  (`33_fxaa.js`).
- `SSGI.fx` - an MGE XE post shader (HBAO + SSGI): **vtastek**'s SSGI, modified
  by **Remiros**; GPL-2.0 like the other MGE XE shaders (the SSGI mode in
  `29_ssao.js`).

**Gardenfell** (Robin Hjelte, MIT; see above) - the page itself, including Robin's
camera smoothing from his own MWSE mod, and his go-ahead for the SSAO port.

**Ours** (StaticNation, GPL-2.0 as part of the page) - the `*_wg_*.js` files,
`23_viewer_profile.js`, `24_ori.js` and `viewer_only.html`; `viewer-shell/MANIFEST.md`
lists each.

The water's other sources - the shore surf (OWSE, mod 56186) and the sewer waves
(Krokantor's shader, mod 45432) - are in the two sections that follow.

## OpenMW Water Shader Enhanced (OWSE) - Vegetto (Nexus Mods, Morrowind mod 56186)

The cell viewer's water carries this mod's shore foam (the surf) and procedural
caustics in the shallows (`viewer-shell/ui/src/26_water.js`), ported from its
`water.frag` - **Vegetto**'s own foam and caustics functions only, not the OpenMW
water shader they sit in. Used under the mod's
permission: "Feel free to use this shader as a resource for your own projects but
for the fog part you must ask Epoch." The fog is not used.

## Enhanced Water Shader for MGE XE - Krokantor (Nexus Mods, Morrowind mod 45432)

The cell viewer's sewer waves - the rings spreading from the sewer outlets of Vivec
and Molag Mar, added to MGE XE's close wave normals - are this shader's, ported to
GLSL in `viewer-shell/ui/src/26_water.js` (`sewerWaves`): its outlet positions and
ring formula, from version 2.0 (the Green-Blue variant). By **Krokantor**, on MGE
XE's water shader; credits as its readme gives them: **vtastek** (peak fix, improved
caustics, optimized sewer waves, bits of code and general help), **phal** and
**harnlarnm** (original foam code), **abot** (sewer waves port), **Hrnchamd** (MGE
XE). Permission: "You can do with this shader what you want as
long as you give proper credit to the original authors and me."

## Wonders of Water - NullCascade (Nexus Mods, Morrowind mod 52815)

**MIT** ("MIT licensed. Do what you want with it."), notice at
`License/Wonders-of-Water/LICENSE`. An MWSE mod that makes MGE XE's water react to the
world; the viewer simulates it (MWSE cannot run under OpenMW) behind the Preview's
**Wonders of Water** switch. Its per-weather multipliers on the wave height and the
caustics, verbatim from its `config.lua` (`25_sky.js`, applied in `26_water.js` and
`35_underwater.js`) - driven by the Preview's Weather picker; still water in a true
interior (its `interop.lua`); and under the water the fog closing in and the water
darkening with depth (`depthFactor = saturate(depth / 1500)`, fog times
`max(1 - depthFactor, 0.1)`, color times `1 - depthFactor`; `06_gl.js`). Its
player-only rules - vampire sun damage, Night Eye - are left out.

## And of course

- **Bethesda Game Studios** - for *The Elder Scrolls III: Morrowind*.
- The wider **OpenMW and Morrowind modding community** - for decades of tools,
  documentation, and reverse-engineering that everything here depends on.

---

*Wraithguard Toolkit is provided as-is. Where we reproduce MIT-licensed material
(notably tes3lint's evil-GMST table, and the algorithms ported from Merged
Lands and yampt), the original copyright and license notice travels with it in
the source. GPL source appears only in the cell viewer's page (`viewer-shell/ui`),
which is itself GPL-2.0, carries the full license text, and is kept apart from the
MIT code; MWSE and OpenMW were read for cross-checking only, and the unlicensed
community Perl scripts contributed ideas, not code.*

*Attribution is something we would rather over-do than get wrong. If anything
here is inaccurate - a name, a license, a claim about what we derived from
whom - please tell us and it will be corrected.*
