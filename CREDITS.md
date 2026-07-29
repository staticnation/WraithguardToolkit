# Credits & Acknowledgements

MLOX Subset Sort stands on the work of a lot of other people. This tool exists
because these projects were generous enough to share their code, formats, and
research. Huge thanks to everyone below — the good ideas are theirs; any bugs are
ours.

If you are one of these authors and want your attribution changed (or removed),
please get in touch and we'll fix it right away.

---

## Code ported or adapted from (MIT-licensed)

These projects are MIT-licensed. We ported logic and/or cross-referenced their
implementations; their copyright notices are reproduced with the relevant parts
and their `LICENSE` files are included in their source folders in this repo.

- **mlox** — © 2009–2017 John Moonsugar (alias), dragon32, Arthur Moore. MIT.
  The load-order rule engine and the rule databases (`mlox_base.txt` /
  `mlox_user.txt`) this whole tool is built around. Our matching, ordering, and
  `[Conflict]/[Requires]/[Note]` predicate logic is a port of mlox's. Several
  of our **Lint** checks (evil GMSTs, the interior fog-density-0 bug,
  expansion-function dependencies) come from mlox's `tes3lint`, credited
  separately below. (The missing-pathgrid check does *not* come from mlox —
  see the unlicensed-scripts section.)
- **tes3lint** — © 2009 John Moonsugar. MIT. Distributed as part of mlox.
  A diagnostic tool for TES3 plugins. Our native **Lint** feature reimplements
  its useful checks against plugin binaries (so they see the whole OpenMW
  multi-folder VFS, with no Perl needed). One thing is *reproduced rather than
  reimplemented*: the table of **72 "evil GMSTs"** — the exact name/value pairs
  an old Construction Set wrote when run without both expansions. Those values
  are research, not something we could rederive, and the table carries John
  Moonsugar's copyright notice inline at `_EVIL_GMSTS` in the engine.
- **[mlox-rules](https://github.com/DanaePlays/mlox-rules)** — maintained by
  DanaePlays and contributors. The **actively-updated** rule database that
  modern mlox (v1.1+) and plox both use. Our "Update Rules..." button downloads
  the current `mlox_base.txt`/`mlox_user.txt` from this repo.
- **plox** — © 2024 Moritz Baron. MIT.
  A Rust reimplementation of mlox. Used as a second reference to harden our
  engine (wildcard/`<VER>` matching, order transitivity, predicate functions).
- **tes3conv** — © 2025 Greatness7. MIT.
  Converts Morrowind plugins ↔ JSON. Used (optionally, if present on PATH) as the
  exact record-identification and field-diff engine behind Check Conflicts.
- **momw-configurator** — © Modding-OpenMW.com (johnnyhostile). MIT.
  We read its `cfg/custom.go` to reimplement its customization-apply logic
  faithfully, so the **Export preview** can simulate exactly what the
  Configurator will do to your `openmw.cfg` (matching, insert/replace/remove
  order, ambiguity errors) before it runs.
- **modmapper** — © 2023 Michiel. MIT.
  The inspiration and reference for the cell-map heatmap (which mods touch which
  exterior/interior cells).
- **Tes3EditX** — © 2023 Moritz Baron. MIT.
  Referenced for TES3 record handling and conflict-resolution UX.
- **TES3Tool** — © 2019 SaintBahamut. MIT.
  Referenced for the TES3 binary record/subrecord layout used by our built-in
  parser.
- **MWEdit** — © Dave Humphrey and contributors. MIT.
  Its `data/Functions.dat` and `mwedit/script_defs.h` are the primary source
  for our script-bytecode opcode table: the function names, their opcode
  values, and the parameter-flag words that describe each operand's encoding.
  Without it the **Bytecode** view in the diff window would be guesswork —
  and guesswork is exactly what we refused to ship.

  The compiler-internal opcodes that no function table lists (notably
  `_SetReference`, emitted for `id->Func`) were **measured from a corpus of
  real compiled scripts** rather than taken from anyone's source — an opcode's
  numeric value is a fact about the game's own data files. `tools/gen_opcodes.py`
  regenerates the table and documents each derivation.

## abot's tes3cmd scripts (idea credited, no code used)

- **`missing_pathgrids.pl`** and **`cell_conflicts.pl`** — © **abot**.
  Published as *Missing Pathgrids* and *Cell Conflicts* on abot's own site,
  ["Morrowind is Home"](https://abitoftaste.modlist.x10.mx/morrowind/index.php?option=downloads&catid=58&Itemid=50&-Morrowind-tools)
  (Downloads → Morrowind tools), alongside MMOG, MRS and abot's other tools.

abot is behind a great deal of what makes Morrowind still worth playing —
Water Life, Silt Striders, the merged-object and resource-scanning tools that
half the community's load orders depend on. These two `tes3cmd --program-file`
scripts are small by comparison and easy to overlook, so: thank you.

**The files themselves carry no copyright line and no licence text.** No
licence granted means the author keeps all rights, which makes these the most
restricted inputs in this project rather than the least. So we treated them as
*read-only inspiration*: **no line of either script is in this tool**, and both
of our implementations were written from scratch in Python against plugin
binaries. What we took is the diagnostic idea, which copyright does not cover.

- *Missing Pathgrids* — the idea: an interior cell with no `PGRD` record is a
  bug, because NPCs cannot pathfind there. Our `[NO PATHGRID]` check
  deliberately *diverges*: the original only considers plugins earlier in the
  load order and so reports false positives, whereas ours accepts a pathgrid
  contributed by **any** plugin.
- *Cell Conflicts* — the idea: "show me every mod touching the same cells as
  this one." That became the **Focus on mod** filter in our cell map (whose
  implementation follows *modmapper*, MIT).

abot — if you would rather we credit this differently, drop the mention, or not
reference your scripts at all, say the word and it is done.

## Approach referenced (no code copied)

- **TES3 Conflictsolver Editor** — ©2026 kirgan 
  (a Mini-TES3Edit–style patch tool). No license file is
  distributed with it; **no code was copied**. We credit it for the field-level
  record-diff *approach* that inspired our field comparison view. All rights
  remain with its author.

## The NIF reader, and why it is written rather than imported

`mlox_subset/nif/` reads Morrowind meshes with our own code. That is a licence
decision, taken deliberately and recorded here so it is not revisited by
accident:

- **pyFFI** — LGPL. The obvious Python choice. This tool ships as a PyInstaller
  onefile binary, and statically bundling an LGPL library carries a relinking
  obligation that does not fit that distribution.
- **nifly** — GPL-3.0. **io_scene_mw** (Greatness7's Morrowind Blender plugin) —
  GPL-3.0, Python, and scoped to exactly this game, which makes it the most
  tempting option by far. **NifSkope** — GPL.
- **nif.xml** (the NifTools format description) — in a GPL-3.0 repository whose
  own licence status is [disputed upstream](https://github.com/niftools/nifxml/issues/86).
  An unresolved licence is worse than one that clearly says no, so it was not
  used as a source either.
- **niflib** — **BSD-3**, and therefore the permissively-licensed reference to
  consult if a layout ever needs checking against an implementation.

## three.js — bundled, not merely referenced

`mlox_subset/nif/assets/three.cjs` is **three.js r185, unmodified**, MIT
licensed, with its licence text beside it as `three-LICENSE.txt`. It is the
first third-party *source* this project ships, as distinct from the Python
packages PyInstaller already collects, so it is called out here rather than
left to be discovered in a build.

It is the **CommonJS** build, which looks like an odd choice until the
constraint is stated: modern three.js ships ESM only, split across
`three.module.min.js` and `three.core.min.js`, and **ES module scripts do not
load from `file://`** — the origin is `null` and the CORS check fails. The
viewer pages are written to disk and opened in a browser, so no ESM packaging
can work. The CJS build is one self-contained file with no `require()` of its
own and runs as a classic script behind a three-line shim.

The orbit controls in the page are ours, not three.js's `OrbitControls.js`,
because that imports the bare specifier `'three'` and would pull ESM back into
a page built specifically to avoid it.

`mlox_subset/nif/bsa.py` reads Morrowind's archives, and is ours for the same
reasons again. **bethesda-structs** (MIT, Stephen Bunn) via **BSAFileExtractor**
(MIT, Pierre GAMBIER) would have been licence-compatible, so this was an
engineering call rather than a legal one: it pulls in `construct`, `multidict`,
`attrs` and `lz4` — the last with a compiled extension — ships a 49 MB tree
covering Fallout and Skyrim record formats this project will never touch, and
every archive in its own test suite is the *post-Morrowind* BSA format, which
shares an extension with Morrowind's and nothing else. Morrowind's layout is a
header and three tables. Neither project was read for the format; it was
implemented from the public description and checked against a shipped archive
with `tools/check_bsa.py`.

`mlox_subset/images/` is ours for the same reasons and by the same method.
Pillow would decode these textures, but it is a large binary dependency in a
PyInstaller onefile build. It was used instead as an **oracle**: the corpus
textures decode byte-for-byte identically to it, BC7 matches on 19,380 random
blocks across every mode and partition, and every PNG we write reads back
through it unchanged. Used but not read, exactly as with NifSkope.

**`pydds` was evaluated for BC7 and rejected on licence.** It is the closest
technical fit — DDS decompression bindings including BC7, which is precisely
what was wanted — and it is **GPLv3-or-later**, which would relicense this
entire project. That decision needed no technical argument at all. Two further
facts made it moot anyway: it *depends on* Pillow rather than replacing it, so
adopting it would have added a dependency rather than removed one; and it is a
compiled extension at version 0.0.8, marked alpha.

`quicktex` (Apache-2.0) would have been licence-compatible and remains the
option if a hand-written BC7 decoder ever proves too slow. It was not needed:
ours matches an independent implementation exactly, and a viewer decodes one
texture on demand rather than a collection.

The BC7 tables come from the **published format specification** — Khronos's
OpenGL BPTC specification and Microsoft's Direct3D 11 documentation — not from
any implementation. `NIF_PROVENANCE.md` records how that was verified, and why
transcribing six hundred numbers needed a cross-check rather than a unit test.

### Greatness7 relicensed the `es3` library so this project could use it

On 28 July 2026, **Greatness7** — author of the Morrowind Blender Plugin and of
`Greatness7/tes3` — offered to relicense the NIF library inside `io_scene_mw`,
and then did it:
[`cbe18b5`](https://github.com/Greatness7/io_scene_mw/commit/cbe18b558299e14ecd959183e3cf9ea096fe95df)
adds an MIT `LICENSE` to `lib/es3/`. `Greatness7/tes3` was already MIT.

That was an unprompted act of generosity toward a project that had spent months
carefully working around his code, and it is worth naming plainly. The
relicensed library is `lib/es3/` **only**; the rest of `io_scene_mw` remains
GPL-3.0 because Blender requires plugins to be, and this project respects that
line. See `NIF_PROVENANCE.md` for the exact boundary.

Within an hour of reading `tes3`, the cross-check had confirmed this project's
hardest-won layout — the typed bounding box — and found a gap it could not have
found alone: `NiUnionBV`, a bound type no file in either corpus carries, which
this reader would have refused. A second implementation sees what a corpus
cannot.

**How each field layout was actually derived — and what was deliberately not
read to derive it — is recorded in `NIF_PROVENANCE.md`.** That document is the
companion to this one: this section says *why* the reader is ours, and that one
says *where every fact in it came from*, with the worked derivations so they can
be re-run rather than taken on trust.

Going GPL-3.0 was considered seriously: it would unlock io_scene_mw, pyFFI,
nifly and — the bigger prize — **OpenMW**, whose NIF loading, texture handling
and `openmw.cfg` semantics this tool models from the outside. The project stays
**MIT** for now, so none of those were read for the reader's field layouts. They
come from the publicly documented format, checked against real meshes with
`tools/check_nif_layouts.py`.

## Referenced for formats & behavior (GPL — no source copied)

We read these projects to understand file formats and expected behavior. **No
GPL source was copied into this tool**, so no copyleft obligations attach to it;
the credit is one of gratitude and correctness.

- **OpenMW** — GPLv3. The engine that makes modern Morrowind modding possible.
  Referenced for `openmw.cfg` semantics, the `.omwaddon`/`.omwscripts` Lua
  formats, and VFS (`data=`) resolution rules.
- **Mod Organizer 2** — GPLv3. Referenced for the "Data" loose-file conflict
  concept behind our data-path (VFS) resource conflict checker.
- **MWSE** — © NullCascade, Merzasphor, Greatness7 and contributors. **GPLv2.**
  We read `MWSE/OpCodes.h` to *check* our opcode table and found it agreed with
  MWEdit on all 533 opcodes they share. Because MWSE is copyleft and this tool
  is not, **no MWSE source was copied**: the shipped table is built from MWEdit
  (MIT), our own corpus measurements, and `customfunctions.dat`.

  That last file needs saying precisely. `customfunctions.dat` is a **data file
  in MWEdit's own text format**, describing the MWSE / MW-Enhanced script
  functions so MWEdit can compile against them. It is installed by running the
  MWSE updater rather than being part of the MWSE source tree, and it is
  configuration for an MIT tool, not a copyleft header — so reading it does not
  bring GPLv2 obligations with it. It contributes **360 opcodes** the base game
  has no equivalent for, which is what makes an MWSE-scripted mod disassemble
  instead of coming out as raw bytes.

  It spells parameter types symbolically (`Long | String`) where
  `Functions.dat` uses hex flag words, and the mapping between the two was
  **derived, not copied**: the two files describe 106 of the same functions, so
  correlating those pins each symbolic name to exactly one bit value. The result
  matches the `FLAG_*` constants already taken from MWEdit's MIT header, which
  is how we know the derivation is right.

  Where the two files disagree — two renames (`XDrop`/`XDropItem`,
  `XEquip`/`XEquipItem`) and 26 differing operand shapes — **the existing
  MWEdit-derived entry is kept** and the disagreement printed rather than
  resolved silently. 25 of the 26 are the same call: MWEdit says `String` where
  customfunctions says `Long | String` for a filename or object id, and UESP's
  per-function pages document those parameters as strings, which settles it.

  The 26th is a genuine error and is corrected: MWEdit gives
  `XFileWriteFloat` a single float operand with no filename, where
  customfunctions lists two and UESP documents
  `xFileWriteFloat filename (string), value (float)`. Corrections live in one
  small explicit table in the generator, each with its evidence.
- **MGE XE** — GPLv3. Referenced alongside MWSE for the same cross-check; no
  source copied.

## Curated data & tooling

- **[Modding-OpenMW.com](https://modding-openmw.com/) (MOMW)** — the curated mod
  lists, the `umo` installer, the MOMW Configurator, and `plugin-order.yml` (the
  source of truth for which plugins belong to which list). This tool is designed
  specifically to *complement* MOMW lists without ever reordering them.
  Customizations are not supported by the MOMW team.
- **tes3cmd** — © 2016 John Moonsugar. MIT.
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

- **[UESP](https://en.uesp.net/) — *Morrowind Mod:Mod File Format*.** CC-BY-SA.
  The community's reference for the TES3 binary layout, and the source of
  `mlox_subset/tes3fields/schema.py`: 46 record types, their subrecords, whether
  each is required, its declared width, and the named members inside the struct
  ones. What is taken is **format fact** — a `NPDT` is 12 or 52 bytes; its first
  two are a uint16 Level — which describes Bethesda's file format rather than
  anyone's prose about it, and the generated module carries the attribution.

  The schema is what lets the diff window say *what a field is* rather than only
  what its value was, and what backs the **Format reference** view beside a
  record diff. Nothing is guessed: where the tables are ambiguous the schema
  says so (a field with two documented layouts carries neither), and 56 of the
  parsed layouts are checked against the byte counts the same tables declare —
  all 56 agree, which is how we know the parse is right.

  Also the source for the LAND, PGRD and script-record field documentation used
  when writing the binary decoders, alongside the MIT-licensed implementations
  credited above.

## Runtime & optional libraries

- **Python** and **Tkinter/ttk** — the language and GUI toolkit.
- **[tkinterdnd2](https://github.com/pmgagne/tkinterdnd2)** — optional drag-and-drop.
- **[PyYAML](https://pyyaml.org/)** — optional, faster `plugin-order.yml` parsing.
- **[pywebview](https://pywebview.flowrl.com/)** — optional in-app cell-map viewer
  (OS webview).
- **[tkinterweb](https://github.com/Andereoo/TkinterWeb)** /
  **[tkhtmlview](https://github.com/bauripalash/tkhtmlview)** — optional inline
  HTML rendering fallbacks.

## And of course

- **Bethesda Game Studios** — for *The Elder Scrolls III: Morrowind*.
- The wider **OpenMW and Morrowind modding community** — for decades of tools,
  documentation, and reverse-engineering that everything here depends on.

---

*MLOX Subset Sort is provided as-is. Where we reproduce MIT-licensed material
(notably tes3lint's evil-GMST table), the original copyright and licence notice
travels with it in the source. We copy no GPL or unlicensed source: MWSE and
OpenMW were read for cross-checking only, and the unlicensed community Perl
scripts contributed ideas, not code.*

*Attribution is something we would rather over-do than get wrong. If anything
here is inaccurate — a name, a licence, a claim about what we derived from
whom — please tell us and it will be corrected.*
