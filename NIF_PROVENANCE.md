# How the NIF reader was written, and where every fact in it came from

This document exists so that the origin of `mlox_subset/nif/` can be examined
by someone who did not write it, including someone who assumes the worst. It
records what was consulted, what was deliberately not consulted, what method
produced each field layout, and what evidence was required before a layout was
accepted.

`CREDITS.md` records the *licence decision* — why the reader is written rather
than imported, and which libraries were ruled out. This document records the
*method*. The two are meant to be read together.

**This is not legal advice, and no part of it should be relied on as such.** It
is a factual record of engineering practice, written by the people who did the
work, so that a lawyer or a maintainer has something concrete to evaluate
instead of a claim.

## What this is, and what it is not

It is worth being exact, because the phrase "clean room" is a term of art and
this project does not meet its strict definition.

A textbook clean-room implementation uses **two isolated teams**. One examines
the original work and writes a functional specification containing no
expression from it; the other, having never seen the original, implements from
that specification alone. The isolation is the point: it produces a documented
chain showing the implementers *could not* have copied, because they never had
access.

**That is not what happened here.** There is one author. There were no two
teams and no wall.

What this project did is narrower and should be described as what it is: an
**independent implementation derived from public documentation and from direct
observation of files the user lawfully owns**, with a deliberate and recorded
policy of not reading the source code of existing implementations whose
licences are incompatible with this project's.

Claiming more than that would be the wrong thing to do in a document whose only
value is that it can be trusted. The protections this practice actually relies
on are:

- **A file format is not itself a copyrightable work.** What is protected is
  the *expression* in a particular implementation — its code, its comments, its
  structure — not the fact that a 32-bit integer sits at a given offset.
  Reimplementing a format from facts is ordinarily lawful; copying someone's
  code that reads it is not.
- **Facts observed from a file you lawfully possess are your own
  observations.** Every byte offset in this reader was confirmed against
  Morrowind meshes on the user's own installed copy of the game.
- **The policy of not reading incompatible sources** means the question of
  copying does not arise on the merits, rather than being defended after the
  fact.

These are general principles, they vary by jurisdiction, and they are not a
substitute for advice from a lawyer.

## Sources that were used

| Source | Licence / status | What was taken |
| --- | --- | --- |
| *Notes for Modmakers* (`morrowind-nif.github.io`) | Community documentation, published as prose for modders | Which block types the Morrowind engine supports; what blocks are *for*; the enumeration used as the coverage denominator |
| The user's installed Morrowind meshes (7,343 files) | Lawfully owned copy of the game | Every byte offset and field width, by direct observation |
| The *Notes for Modmakers* attachment packages (see below) | Distributed by the same community project as teaching material | Files exercising specific block types in isolation |
| NifSkope, as an **instrument** | GPL-3.0 program, used but not read | Confirmation that a parse result matches what a known-good viewer shows |

## Sources that were deliberately not used

Recorded so that the absence is visible, not merely asserted. See `CREDITS.md`
for the licence analysis behind each.

- **`nif.xml`** (NifTools' machine-readable format description). Not used as a
  source for any field layout. It lives in a GPL-3.0 repository whose own
  licence status is [disputed upstream](https://github.com/niftools/nifxml/issues/86),
  and an unresolved licence is worse than one that clearly says no.
- **NifSkope's source code**, **nifly**, **NiflySharp**, **io_scene_mw**,
  **OpenMW**. All GPL-family. None read for field layouts.
- **pyFFI**. LGPL; ruled out for distribution reasons rather than reading
  reasons, but likewise not read for layouts.
- **niflib** is BSD-3 and would be a permissible reference. It has not been
  needed and has not been consulted. If it ever is, that use should be recorded
  here at the time, not reconstructed afterwards.

### The specific line drawn around NifSkope

NifSkope is GPL-3.0 and this project is MIT, so the distinction matters and is
applied consistently:

- **Permitted — NifSkope as an oracle.** Opening a file the user owns and
  observing *what the file contains*: how many blocks, in what order, of what
  types, with what values. These are facts about the user's file. Running a
  program does not place its licence on your observations.
- **Not permitted — NifSkope as a source.** Transcribing the field names,
  types and orderings it *displays* for a block this project has not yet
  implemented. That display is generated from `nif.xml`, so copying it would
  import exactly the artefact ruled out above, by a longer route.

In practice this means NifSkope answers "is this parse right?" and never "what
are the fields?". The one time it was used during this work — settling whether
`c/amulet_common_1.nif` contains one `NiMaterialProperty` block or two — was
purely the first kind: counting blocks in a block list.

## The method

A layout is a hypothesis, and a hypothesis about a binary format is testable in
a way that leaves very little room for opinion. NIF 4.0.0.2 blocks carry **no
length field**, and each is preceded by a length-prefixed type name. So a
correct layout consumes exactly the right number of bytes and the cursor lands
precisely on the next type name; a layout wrong by even one byte lands on
something that is not a type name at all.

That property is the entire method, and it is unusually strong: it means a
guess cannot quietly half-work.

1. **State the hypothesis from public documentation and from the shape of the
   data.** What fields a block plausibly holds, in what order.
2. **Walk real files.** `tools/check_nif_layouts.py` reads a folder and reports
   how far it got in each file.
3. **Require the walk to land exactly.** If the next four bytes read as a
   length `n` and the following `n` bytes read as a well-formed type name, the
   layout consumed the right number of bytes. If they do not, it did not.
4. **Cross-check the whole file against a scan that shares no code with the
   reader.** `mlox_subset/nif/scan.py` recovers a file's block list *without
   using any field layout*, so it cannot fail the way the reader fails. It also
   reconciles against the block count the file's own header declares, and
   disqualifies itself when it does not. `--verify` reports the first index at
   which the two disagree.
5. **Only then accept the layout**, and record how it was derived in a comment
   at the layout itself, not only here.

### The evidence standard, stated as a rule

> A field is added, removed or resized only when doing so makes the reader land
> exactly on the following type name in real files, and the layout-free scan
> agrees with the reader across the whole file.
>
> Adding fields speculatively and in bulk is not permitted, because a layout
> guessed wrong does not fail where the guess was, and a batch of them makes
> the next survey unreadable. One at a time, each validated.
>
> **The files that already parse are evidence too.** A derivation that explains
> every failure in front of you is not thereby correct, and the working files
> are the ones nobody thinks to re-check. See the bounding box below, where
> exactly this went wrong.

## Worked examples

These are recorded in full because the reasoning is the evidence. Anyone can
re-run them.

### `NiGeomMorpherController` — one byte longer than its siblings

Every file that lost alignment stopped with a type name of `\x00NiMorphData`:
a leading NUL followed by the correct name. That is precisely what a cursor one
byte early looks like — it read the last byte of the true length prefix as the
first byte of the name.

Inspecting the bytes at the stop point in `b_n_nord_f_head_01.nif`:

```
bytes at the failure point: b'\x00\x0b\x00\x00\x00NiMorphData\x03\x00\x00\x00'
u32 at q+1 = 11 -> name: b'NiMorphData'
```

Consuming one more byte in the preceding block makes the next `u32` read 11 and
the next 11 bytes read `NiMorphData`. All 10 alignment failures in the sample
had `NiGeomMorpherController` as the preceding block, and the byte was `0` in
every one. After the fix, alignment failures across all 7,319 vanilla meshes
went to **zero**.

Because the corpus shows only the value `0`, its *meaning* is not determinable
from the evidence. It is therefore named `trailing_flag` — a name that records
where it sits and declines to invent what it does. Naming it after a guess
would have been the moment this stopped being observation.

### `NiBillboardNode` — a field that is not there

An earlier layout carried a speculative `billboard_mode` `u16`, reasoning from
later NIF versions. `--explain` on `BM_Snow_01.NIF` showed the reader finishing
two bytes past the next type string. The field was removed. Morrowind's
billboarding has no mode to select, which is consistent with the observation
but was *not* the reason for the change — the byte count was.

### `NiKeyframeData` — a field that is

The reverse case: a float was missing before the three XYZ key groups. With it
consumed, the walk lands exactly on `NiTextureEffect`.

### `vertices` and `normals` — a bug caused by inferring rather than recording

Both fields are the same *kind*, and the reader originally inferred the
presence-gate from the kind. That meant normals were read whenever vertices
were present, desynchronising every mesh that had one without the other. Gates
are now written out explicitly per field. This is recorded because it shows the
failure mode the method is designed around: the bug did not fail where the bug
was.

### The particle controller — a fixed head, found by five counts agreeing

`NiParticleSystemController` block bodies measured 40154, 6154 and 16154
bytes. Each is 154 bytes plus a multiple of 40, and the multiplier — 1000, 150,
400 — appears in the block as a `u16` at offset 137. Across 51 fixtures with
five distinct counts, every body is exactly `154 + count * 40`.

Two conclusions follow from the arithmetic rather than from any document.
First, the record size is 40 bytes: five different counts fitting exactly
leaves no room for coincidence. Second, **nothing follows the array**, because
a trailing field of any size would offset all 51 by that amount.

111 of the head bytes are emitter parameters that were not individually
identified. They are stepped over as a measured span called
`emitter_parameters`, not given invented names. The width is what the rest of
the file depends on; a plausible-looking wrong field name is worse than an
admitted gap, because it would be believed and repeated.

### Particle rotations — an optional array found by subtraction

Two `NiRotatingParticlesData` fixtures both declare 1000 particles. One block
is 32052 bytes and the other 48052. The difference is exactly 16000 — sixteen
bytes per particle — which identified an optional array behind its own flag.
Six fixtures spanning 12 to 1000 particles and every combination of the
optional vertex, colour, size and rotation arrays then reconcile to the byte.

### `NiTextureEffect` — counted entries that are not links

The block's tail is `4 + 4n + 91` bytes, where `n` is the leading count: the
four observed shapes are 95, 99, 111 and 115 bytes for counts of 0, 1, 4 and 5.

The counted entries hold values such as `0x0b741950`. Those are not block
indices — they are memory addresses the exporter left in the file. They are
counted and stepped over rather than exposed as links, because a caller
following them would be following pointers into a process that exited two
decades ago. This is recorded because the honest reading of the bytes and the
convenient one differ here, and the honest one was taken.

### The bounding box — where this method was applied badly, and what it cost

Three mod meshes failed inside an optional bounding box. Bounding the block
between its start and the next type name gave one span that landed exactly, in
both files it could be computed for: **20 bytes**. That was written into the
reader as established, with the derivation spelled out in a comment.

It was wrong. Twenty bytes broke thirteen files that the previous width read
correctly — files that had never been re-examined because they were not
failing.

The box is *typed*. The word after the presence flag selects the size: type 1
carries a translation, a 3x3 rotation and an extents triple, and type 0 carries
sixteen bytes. Across every block in the corpus that sets the flag **and
parses**, the type word is 1 in all 27; in the meshes that would not parse it
is 0 in every one. No single width can be right for both populations, and
averaging them produces a number that is right for neither.

This is recorded at length because the failure was in *method*, not in
arithmetic. Every measurement was correct. The step that was skipped was the
one the standard above already required: re-running the whole corpus before
accepting the change. An unrecognised box type is now refused outright rather
than guessed, because an unknown width does not fail where the guess was.

### Property block counts — where an external reference was wrong

A per-file block census shipped with one of the documentation packages was used
as a reference and reported far fewer property blocks than the reader found.
Three independent methods were applied:

- the layout reader: 2 `NiMaterialProperty` in `c/amulet_common_1.nif`;
- the layout-free scan: 2;
- NifSkope's block list: blocks 4 and 11, both `NiMaterialProperty`.

The census said 1. It undercounts shared property blocks specifically, while
matching on every other type. The reader was correct and the reference was not,
which is why the project no longer depends on that file: `--verify` generates
its own reference by scanning.

## What the method produced

Stated so the claim can be checked rather than taken on trust. Against the
7,343 meshes of a vanilla Morrowind install, using `--verify`, which compares
the layout reader against the layout-free scan:

```
identical     : 7339
unverifiable  :    4
No divergence: every block the reader named matched the scan exactly.
```

Zero files stopped early and zero diverged. The four marked unverifiable are
ones where the *scan* found more blocks than the header declares — its known
false-positive mode, where a node happens to be named like a type — so they are
excluded from the comparison and checked against the header count instead.

The reader implements 54 block types, which covers every type occurring in
those meshes. Every layout in it was derived by the method
above, and the four blocks whose fields could not all be identified carry
measured spans with names that say so (`emitter_parameters`,
`unidentified_tail`, `path_parameters`, `projection`) rather than guesses.

### Where the method stops: a file that is simply wrong

The exact-landing test assumes the file is well-formed. One mesh in the sample
is not, and it is worth recording because it marks the method's boundary.

`dbs_meatstick.nif` declares 26 blocks and the layout-free scan finds exactly
26 type names, so every block *boundary* is where it should be. Block 10 then
reads a property count of `0xFFFFFFFF`. Opened in NifSkope — as an oracle, per
the rule above — it shows orphaned blocks. The boundaries are sound and the
contents are not.

No layout can be derived from it, because there is no consistent layout to
find: the bytes do not describe a valid block. The correct response is to
refuse the file and say so, not to widen a layout until it swallows the
nonsense. A layout invented to fit one broken file breaks the sound ones, which
is exactly what happened once already with the bounding box above.

**So a failure to parse is evidence about the reader only when the file is
sound.** Distinguishing the two needs something outside the bytes — here, a
person opening it in a viewer.

## The same method, applied to textures

`mlox_subset/dds/` decodes DDS textures under the same rules and for the same
reasons. The block formats are arithmetic -- a DXT1 block is two 16-bit colours
and sixteen 2-bit selectors, and the decode is the interpolation the format
defines -- so they were implemented from the public description and checked
against real files.

The cross-check here was **Pillow**, used as an instrument and not read: all 50
textures in the local corpus decode byte-for-byte identically to it, and every
PNG the encoder writes reads back through it with the exact pixels it was
given. Pillow is not a dependency of this project. The distinction is the same
one drawn around NifSkope above -- comparing our output against another
program's output is an observation, not a copy.

## What a modded corpus changed

The figures above are for a vanilla install. They were also, for a while, the
basis of a claim that four block types were not worth implementing because
vanilla never uses them.

A run over 80,197 meshes from a mod collection disproved that, and is recorded
here because it bears on how coverage claims in this document should be read:
**vanilla is not the population this tool serves.** The reader now accepts NIF
4.0.0.0 as well, which was established the same way as every layout -- 40 such
files had their version word alone rewritten and every one then parsed
identically to the layout-free scan, so the header differs and the layouts do
not.

## The corpus

Two distinct packages are present, from the same community documentation
project but distributed separately. They are **not the same set** and are
easily confused, so the difference is recorded:

| | `NifCorpus/` (outer) | `NifCorpus/Nif_files_Examples/` |
| --- | --- | --- |
| `.nif` files | 365 | 191 |
| Also contains | 139 `.flv` and 30 `.mp4` tutorial videos, 51 `.max` scenes, 50 `.dds` textures, 15 `.txt`, 7 `.gif`, 1 `.kf` | 4 `.txt`, 4 `.png`, 2 `.kf`, 1 `.gif` |
| Organisation | Tutorial material; **not every folder contains a `.nif`** | Folders named for the block type they demonstrate |
| Total `.nif` recursively | **556** (includes the 191 below) | 191 |

Two practical consequences, both of which have already caused bugs:

- **556 is the recursive total**, so a count taken over `NifCorpus/` already
  includes `Nif_files_Examples/`. Reporting them as 556 + 191 would double-count.
- **318 of the 556 use an uppercase `.NIF` extension.** File discovery must be
  case-insensitive; a `rglob("*.nif")` silently missed them on Linux.

Neither package, nor any sampled game asset, is redistributed by this project.
`tools/check_nif_layouts.py --collect` copies real meshes out of the user's game
install for local analysis, and both that destination and the generated reports
are listed in `.gitignore`. The game files are the user's own; they stay on the
user's machine.

## Rules for anyone continuing this work

1. Do not read `nif.xml`, NifSkope, nifly, NiflySharp, io_scene_mw or OpenMW for
   field layouts. If you have read them, say so, and do not write layouts.
2. Use NifSkope to check answers, never to obtain them.
3. Derive from bytes in files you lawfully own, and require the exact-landing
   test plus scan agreement before accepting a layout.
4. Name a field after what the evidence supports. `trailing_flag` is a better
   name than a confident guess, because the name is part of the record.
5. Record the derivation in a comment at the layout, and add a worked example
   here when the reasoning is not obvious from the comment.
6. If a permissively-licensed reference such as niflib is ever consulted, record
   it here **at the time**.
