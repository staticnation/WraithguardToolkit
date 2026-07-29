# Changelog


## 3.1

Everything after the 3.0 release. 3.0's own entry below is exactly as it
shipped, so the two can be told apart at a glance, and
`MloxSubsetSort-3.0 release/` is the backup of what actually went out.

The headline is that the emitted TOML could abort a Configurator rebuild
outright (see **Fixed**), which is worth reading before anything else here.
Otherwise: the rule maker now writes every rule the format has and ships a
reference for it, the 3D terrain view was drawn 55x too steep and is now to
scale with its shading fully exposed, and the GUI has 45 automated checks where
it had none.

### Added

- **A lit material view in the texture comparison.** A flat side-by-side view
  cannot compare two normal maps at all — a picture of one is a field of pale
  blue, because what it encodes is how a surface catches light. So each texture
  is also drawn on a lit quad with its own `_n`/`_nh` and `_spec` siblings
  applied, under one light the user can drag, with sliders for key and ambient
  intensity.

  Checkboxes turn diffuse, normal and specular off independently, so the
  lighting stays fixed while the maps come and go and the thing that changes is
  the map rather than the whole scene. Each toggle appears only where such a
  map exists, since a permanently dead control implies a broken feature.

  Two details that are the opposite of the obvious choice: the camera is
  **orthographic**, because a perspective one foreshortens the two quads
  differently and makes them disagree for a reason not in the files; and normal
  maps are loaded **linear, not sRGB**, because reading a field of vectors as
  colour bends every one of them before use.

- **Texture comparison** (`mlox_subset/images/compare.py` and `viewer.py`).
  The conflict scan could already say two mods ship the same texture path; it
  could not say whether that mattered. Now it can, with three views because
  each answers a different question: *side by side* ("which do I prefer"),
  *overlay with a wipe* ("what moved" — the eye detects motion far better than
  it compares two things it must look back and forth between), and *difference*
  ("where is the change", the only one that shows a change too small to see).

  Four decisions in it are worth stating, because the obvious alternative is
  wrong in each case:

  - **Different sizes are a finding, not a failure.** A retexture that doubles
    the resolution is the commonest texture conflict in this game. Nothing is
    rescaled — resampling would invent pixels and then report differences in
    the pixels it invented.
  - **Two numbers, not one.** A re-compression nudges nearly every pixel by a
    level; a real retexture moves a few pixels a long way. A single mean ranks
    the first above the second, so the share of pixels changed and the worst
    single-channel move are both reported.
  - **A one-level difference is not a change.** Every tool that touches a DDS
    rewrites it slightly. With a threshold of zero, every recompressed texture
    in a collection reports as 100% changed — true and useless.
  - **Roles are checked before pixels.** Comparing a normal map against a
    diffuse map produces a large, confident, meaningless number.

- **Nineteen more NIF block types**, and a licence worth recording. Greatness7
  relicensed the `es3` library inside `io_scene_mw` under MIT
  ([`cbe18b5`](https://github.com/Greatness7/io_scene_mw/commit/cbe18b558299e14ecd959183e3cf9ea096fe95df))
  after offering to, and `Greatness7/tes3` was already MIT. Reading them found
  two things within the hour:

  - **Confirmation** of this project's hardest-won layout. The typed bounding
    box — where a confident conclusion drawn from two files once broke thirteen
    meshes that already worked — matches `tes3` exactly on both type numbers
    and both widths, derived independently from bytes.
  - **A gap no measurement here could have found.** `NiUnionBV` is a *recursive
    list* of bounding volumes rather than a fixed width, and no file in either
    corpus carries one. A type absent from the corpus produces no failure to
    count, so 100%-of-vanilla and 99%-of-mods said nothing about it.

  Nodes, lights, controllers, triangle strips and more followed, taking the
  categorised NIF sample archive from 624 of 768 files to **754 of 768
  (98.2%)** and the reader from 66 known block types to 89. Each is marked in
  `blocks.py` as *taken* rather than *derived*: a derived layout survived the
  exact-landing test across thousands of files, a taken one is a transcription
  confirmed against however many samples carry that type — sometimes one. Both
  true, not equally well-evidenced.

  **The vanilla and mod figures are unchanged at 100% and ~99%**, because every
  one of these types was absent from both.

- **Every texture format Morrowind and OpenMW use now decodes**, still with no
  third-party dependency. `mlox_subset/dds/` has become `mlox_subset/images/`,
  because it is no longer only DDS:

  | | |
  | --- | --- |
  | DDS | BC1, BC2, BC3, **BC4**, **BC5**, **BC7**, uncompressed, and the Direct3D 10 header form |
  | Targa | plain and run-length encoded, colour-mapped, 8 to 32 bits |
  | Bitmap | 1, 4, 8, 16, 24 and 32 bits, palettes, both row orders |
  | PNG | passed through to the browser untouched — it already decodes them better than we would |

  **BC7 was the real work.** A 16-byte block carries one of eight modes, and
  the mode decides everything after it: how many colour subsets the block is
  cut into, how wide the endpoints are, whether alpha exists, whether a second
  index set exists, whether a channel was rotated into alpha before encoding.
  Nothing sits at a fixed bit offset. It is defined by roughly six hundred
  transcribed table entries, and a single wrong one produces a correct-looking
  image with a handful of wrong 4×4 blocks — invisible to any test written by
  the same person who transcribed the tables.

  So it is checked against a decoder that shares none of those assumptions:
  **19,380 random blocks across all eight modes and all 64 partitions, matching
  Pillow byte for byte**, plus 512 blocks each for the other formats and real
  files from the corpus. Random bits are the harsh test here — every 128-bit
  pattern is a legal BC7 block, so noise exercises endpoint ordering, P-bits
  and anchor index widths far harder than a photograph could. See
  `tools/check_bc7.py` and `tools/check_images.py`.

  **`pydds` was evaluated and rejected on licence.** It is the closest
  technical fit — BC7 bindings, actively the thing this needed — and it is
  **GPLv3**, which would relicense this entire project. It also depends on
  Pillow, so it would have been additive rather than a replacement. `quicktex`
  is Apache-2.0 and would have been permissible, but is a compiled extension in
  a PyInstaller onefile build. Pillow has never been a dependency here and
  still is not: it is the oracle these decoders are checked against, and
  nothing shipped imports it.

- **Textures are classified by role** (`mlox_subset/images/roles.py`), because
  a normal map is not a picture. Three conventions say what a texture is for
  and all three are real: vanilla puts it in the mesh's `NiTexturingProperty`
  slot, OpenMW infers it from file-name suffixes (`_n`, `_nh`, `_spec`,
  `_diffusespec`) configured under `[Shaders]`, and OSG native meshes name it
  on the texture unit outright. All three are read.

  The **bump slot is deliberately not called a normal map**. Vanilla Morrowind
  does not render bump or normal maps at all; MGE-XE and MCP add the capability
  by repurposing the *environment map* slot, and NifSkope follows that
  convention. What a bump slot contains depends on which toolchain wrote the
  file, so it is recorded as its own role rather than guessed at.

  This is what stops a report claiming a conflict between one mod's
  `tx_rock.dds` and another's `tx_rock_n.dds`, which are complementary channels
  of one material rather than rivals.

- **BC5 normal maps reconstruct their third channel.** The format stores only X
  and Y, because a unit vector's Z follows from them. Leaving blue flat would
  make two genuinely different normal maps compare as identical whenever they
  happened to share X and Y — and comparing a mod's normal map against another
  mod's is a goal of this project, not an edge case. The **DirectX** convention
  is used, matching both engines: green is *not* flipped. Tooling written for
  OpenGL flips it by default, which would report every normal map as differing
  from a byte-identical copy of itself.

- **Light controls in the 3D mesh view**: key intensity, ambient level, light
  angle, and a follow-the-camera mode. Not decoration — a normal map changes
  nothing under flat ambient light, so without a light that moves there is
  nothing to see. Follow-the-camera is off by default and deliberately so: it
  means moving the camera changes the lighting, so two providers can never be
  compared under identical light while it is on.

- **OpenMW auxiliary maps in the 3D view.** Where a `_n` or `_nh` sits beside a
  mesh's diffuse texture, it is offered as a toggle. These exist in a mod
  collection while being mentioned in no mesh at all — the Morrowind NIF has no
  dependable slot for them, so OpenMW finds them by name. The control appears
  only when the collection actually ships one, because a permanently dead
  control implies a broken feature rather than an unused one.

- **BSA archives are read**, so the base game's own assets resolve. Nearly all
  of Morrowind's textures live inside `Morrowind.bsa`; without this every
  base-game mesh looked untextured and every base-game texture looked missing —
  both false. Loose files still win over archived ones, matching the engine, so
  a retexture mod overrides the archive exactly as it does in play.

  Verified against the shipped archive: **11,090 files indexed, 300 extracted,
  every one starting with the magic its extension implies**. That last check is
  the one that matters — an index can be entirely self-consistent and still
  point at the wrong offset, and the reader's own round-trip test cannot catch
  it because the test writer shares the reader's assumptions.

  Written rather than imported. `bethesda-structs` is MIT and would have been
  usable, but pulls in `construct`, `multidict`, `attrs` and `lz4`, ships 49 MB
  of Fallout and Skyrim record formats, and every archive in its own test suite
  is the *post-Morrowind* BSA — a different format that shares an extension and
  nothing else.

- **A node tree beside the 3D view**, listing what a render structurally
  cannot: collision nodes, controllers, properties, and blocks nothing
  references. Orphans are shown rather than dropped.

- **Textured meshes.** UV coordinates, texture references resolved through the
  data folders and archives, DDS decoded to PNG. A **Textures** toggle turns it
  off, because two versions of a mesh wearing the same texture differ in
  *shape* and the texture hides it.

- **The 3D view opens in the in-app viewer**, like every other visualisation,
  rather than the browser. The viewer chain now understands a URL as well as a
  file, since this page is served rather than written.

### Changed

- **One viewport with toggles, not side-by-side panes.** Separate panes each
  framed their own mesh, which is the one thing a comparison must not do: two
  meshes at different scales look identical when each is fitted to its own
  viewport. The frame now covers every provider whether shown or not, so
  toggling never moves the camera.


- **The 3D view is served over loopback, and can still be exported as one
  file.** Viewing starts a server on `127.0.0.1` and hands the browser an
  **8 KB** page instead of a multi-megabyte document; three.js is fetched once
  and cached rather than re-embedded per view. **Export 3D file...** writes the
  standalone version, which is also the automatic fallback when no port can be
  bound.

  Both come from one builder. The difference is confined to how bytes arrive --
  a sink that either base64s a blob into the document or publishes it as a URL
  — and a test asserts the rendering half of the two pages is byte-identical,
  because a fallback sharing no code with the primary path is a second
  implementation waiting to rot.

  The server is deliberately not a web framework. It binds loopback only, on an
  OS-chosen port, requires a per-session token, and **has no filesystem
  mapping at all**: payloads are registered in memory and served by key, so
  path traversal is not defended against, it is absent. `fastapi` + `uvicorn`
  was considered and measured — 14 packages, 34 MB and a compiled
  `pydantic_core` extension, against an app that is currently ~38 MB — to serve
  a fixed dictionary of blobs to one local browser.


- **A 3D mesh viewer.** Select a conflicting `.nif` in the Resource Conflicts
  window and press **View in 3D**: both meshes open side by side, orbitable,
  in one self-contained HTML file.

  three.js r185 is vendored unmodified with its MIT licence beside it. It is
  the **CommonJS** build, which is not the obvious choice and is the only one
  that can work: modern three.js ships ESM only, split across two files, and ES
  module scripts do not load from `file://` — the origin is `null` and the CORS
  check fails. These pages are opened from disk. The CJS build is one
  self-contained file and runs as a classic script behind a three-line shim.
  The orbit controls are ours, because three.js's own `OrbitControls.js`
  imports the bare specifier `'three'` and would drag ESM straight back in.

  Geometry travels deflated and is inflated by the browser's native
  `DecompressionStream`. Measured on a 204k-triangle mesh: JSON decimals
  5.40 MB, base64 typed arrays 4.91 MB (base64's overhead hands most of the
  binary saving back), deflated typed arrays **1.86 MB**. The two-pane demo
  page went from 12.9 MB to 5.0 MB.

  Also measured, because it was worth asking: embedding the raw `.nif` and
  parsing it in the browser would ship **4.37 MB** against 1.86 — the file
  carries normals, UVs, animation and blocks a viewer never draws — and would
  need a JavaScript NIF parser, which three.js does not have and never got
  (the request is still open from 2012).


- **Mesh conflicts now say what the winner costs you.** When two mods ship the
  same `.nif` and the bytes differ, the resource report and CSV say whether the
  winning mesh loses collision, loses animation, drops to a fraction of the
  triangles, or asks for textures nobody ships.

  This is the point of the whole NIF reader. A conflict list tells you one file
  won; it cannot tell you the winner is a low-poly stand-in with no collision,
  and that is the difference between a list you skim and a list you act on.

  It stays cheap by only opening files that already conflict **and** already
  differ in bytes — a subset of a subset — and by caching on content, so a mesh
  body shipped by four mods is parsed once. It reuses the blake2b digests the
  conflict scan already computes, which matters more than it sounds: caching
  the parses alone left hashing as the dominant cost, five seconds over a
  corpus where every parse was a cache hit.

- **A mesh detail panel that reads nothing until you ask.** Selecting a row in
  the resource window reads that mesh, then and only then, and describes every
  provider — shapes, triangles, textures, collision, animation — plus what the
  winner loses against each. Reselecting is free.

- **DDS decoding with no new dependency** (`mlox_subset/dds/`). BC1, BC2 and
  BC3 plus uncompressed surfaces, decoded to RGBA, and a PNG encoder built on
  `zlib` alone so a onefile build gains nothing to bundle.

  Verified against an independent implementation: all 50 textures in the local
  corpus decode **byte-for-byte identically to Pillow**, and every PNG this
  writes reads back through Pillow with the exact pixels it was given. Pillow
  is not a dependency; it was the oracle, in the same role the layout-free scan
  plays for the NIF reader.

- **NIF 4.0.0.0 is accepted.** It differs from 4.0.0.2 in the header and not in
  the layouts, which was measured rather than taken on report: 40 such meshes
  had their version word alone rewritten and every one then parsed identically
  to the layout-free scan. Refusing them was costing 45 files in one mod
  collection for no benefit.

- **`NiSwitchNode` and `NiLODNode`**, which never occur in vanilla and together
  caused 92% of everything that stopped early in a real mod collection.

### Fixed

- **The served 3D page failed with "THREE is not defined".** The CommonJS
  build needs a shim around it -- globals before, namespace after -- and the
  served path emitted the `<script src>` with neither, so the library ran
  against an undefined `exports`. The shim now wraps the library in both modes.


- **The mesh findings never reached the GUI.** The analysis pass had one
  caller: the command line. The Resource Conflicts window ran the scan without
  it, so the feature existed and was unreachable from the app. It now runs
  there too, and a column marks the rows with a finding (`!`) or an unreadable
  mesh (`?`) so they are visible without selecting anything.


- **UV data follows the set count, not the `has_uv` flag.** Meshes carrying
  `num_uv_sets=1` with `has_uv=0` write the UV data anyway; trusting the flag
  skipped it and desynchronised the rest of the block. Confirmed by an
  invariant rather than by "it parses": with the count as the gate,
  `num_triangle_points` comes out as exactly three times `num_triangles`.

- **The bounding box is typed, not fixed-width.** Type 1 carries a full
  transform (64 bytes); type 0 carries 20. Found by separating the two
  populations — the type word is 1 in all 27 blocks that parse and 0 in every
  mesh that would not — after a first attempt that took a single 20-byte width
  from the failing files alone and broke 13 that had been working. An
  unrecognised type is now refused rather than guessed.

- **A miscounted total in `--verify`.** "unverifiable but incomplete" is a
  subdivision of "unverifiable", not a category beside it, and the total summed
  the tally — so a run over 80,197 files reported 81,026. A total that
  disagrees with its own parts quietly discredits every other number in the
  report.


- **The NIF reader now reads every mesh Morrowind ships.** Against the 7,343
  meshes of a vanilla install: **7,339 identical to a layout-free cross-check,
  0 stopped early, 0 diverged.** It was at 85.5% when this run started.

  The four exceptions are not files the reader struggles with. They are files
  where the *cross-checking scan* finds more blocks than the header declares —
  its known false-positive mode, where a node happens to be named like a type —
  so it cannot serve as a reference for them. They are now checked against the
  header count instead of being skipped, because excluding a file behind a
  limitation of the check rather than of the reader teaches nothing.

  Twenty-one block types were added to get there, one at a time, each confirmed
  by landing exactly on the following type name *and* by agreeing with a scan
  that uses no layout knowledge at all. Skinning (`NiSkinInstance`,
  `NiSkinData`), morph targets, the twelve particle types, texture effects, UV
  animation, cameras, path controllers and embedded images.

  Every layout was derived by reconciling block lengths, never by assumption,
  and the derivations are recorded at the layouts and in `NIF_PROVENANCE.md`:

  - `NiParticleSystemController` is a fixed 154-byte head plus `count * 40`,
    confirmed across 51 fixtures with five distinct counts. That also proves
    nothing follows the array, since a trailing field would offset all 51.
  - Two `NiRotatingParticlesData` fixtures with 1000 particles differ by
    exactly 16000 bytes, which identified an optional 16-byte-per-particle
    rotation array behind its own flag.
  - `NiTextureEffect`'s tail is `4 + 4n + 91` across four observed shapes. Its
    counted entries hold values like `0x0b741950` — exporter memory addresses,
    not block indices — so they are counted and stepped over rather than
    offered as links a caller might try to follow.
  - `NiMorphData` writes its interpolation word even when the key count is
    zero, unlike every other key group in the format. Both readings were run
    against all 26 fixtures: "always written" lands on 26, the alternative on 3.

  Where fields could not be identified from the bytes they are stepped over as
  *measured spans* with names that admit it — `emitter_parameters`,
  `unidentified_tail`, `path_parameters`, `projection`. The width is what the
  rest of the file depends on, and an invented field name is worse than an
  admitted gap because it gets believed.

### Fixed

- **`NiTexturingProperty` truncated any mesh with more than one decal.**
  `texture_count` is a slot count, not a cap of seven. On `7decals.NIF` it
  reads 13, and the reader stopped 156 bytes short — exactly six more slots at
  26 bytes each. This was the worst class of bug in the reader: it stopped
  *inside a block type the reader claims to support*, so it produced
  confidently wrong output rather than an honest gap. 11 files in the corpus.

  `--verify` now separates the two cases explicitly, since a single list of
  stops had been burying these under hundreds of ordinary coverage gaps.


- **A provenance record for the NIF reader** (`NIF_PROVENANCE.md`). Where every
  field layout came from, what was deliberately not read to derive it, and the
  worked derivations in full so they can be re-run instead of taken on trust.

  It is careful not to overclaim. "Clean room" is a term of art meaning two
  isolated teams, and this project has one author and no wall, so the document
  says so plainly and describes what was actually done: an independent
  implementation from public documentation and from direct observation of files
  the user lawfully owns, under a recorded policy of not reading incompatible
  sources. A document whose only value is that it can be trusted is the wrong
  place to claim more than happened.

  It also draws the line around NifSkope explicitly — permitted as an *oracle*
  ("is this parse right?"), never as a *source* ("what are the fields?"), since
  its display is generated from `nif.xml`, which `CREDITS.md` rules out.

- **The NIF reader now checks itself against a scan that shares none of its
  code** (`mlox_subset/nif/scan.py`, `tools/check_nif_layouts.py --verify`).

  The layout reader walks a file by knowing how wide every field is, which
  gives it a specific failure mode: one wrong width desynchronises everything
  after it, and the result is not a crash but a plausible wrong answer. The
  scan recovers the block list using no field layout at all, so it cannot fail
  the same way, and the file's own header says how many blocks there should be
  — so a scan that miscounts disqualifies itself instead of misleading.

  It replaces an externally supplied census that turned out to undercount
  property blocks: `x/ex_s_longhouse_blue.nif` really has 53 `NiMaterialProperty`
  blocks where the census recorded 9, confirmed independently by the scan and
  by NifSkope. Beyond being wrong, the census was a file of unclear provenance;
  a reference generated from your own installed game raises no such question and
  works on mod folders too.

  The first version of the scan was wrong and the self-check caught it. Every
  string in a NIF is length-prefixed, so "u32 length then that many bytes"
  matches a node called `Bip01` as readily as a type name, and it over-counted
  522 of 556 files. Adding NIF's naming convention as a second filter took that
  to 553 of 556. That trade is documented in the module: the scan can no longer
  find a type named arbitrarily, only one named the way every type in this
  format is named.

  Against the corpus: 156 files identical, 397 a clean prefix, **zero
  divergences**. `--verify` also separates the two kinds of stop, which a single
  list had been burying — a file stopping on a type the reader *claims to
  support* is a layout bug, while one stopping on an unimplemented type is a
  gap. That split immediately surfaced 11 real bugs under 397 gaps.

### Fixed

- **`NiGeomMorpherController` was one byte short**, and it was the only
  alignment bug in all 7,319 vanilla meshes.

  Every affected file read a type name of `\x00NiMorphData` — the correct name
  behind a leading NUL, which is exactly what a cursor one byte early looks
  like. With the byte consumed the next `u32` reads 11 and the next 11 bytes
  read `NiMorphData`. It is 0 in every observed file, so it is named
  `trailing_flag`: the name says where it sits, not what it means, because the
  corpus does not say what it means.

- **A desynchronised cursor was reported as an unknown block type.** That
  blamed a missing layout for what was a wrong field width, and inflated the
  missing-type count with files that were really layout failures. The two are
  now distinguished, and the missing-type ranking changed as a result.

- **Stop reasons could carry raw binary into logs and terminals.** A
  desynchronised read produces arbitrary bytes, and those were interpolated
  into the message unescaped — one survey printed an embedded NUL and a run of
  high bytes to stdout. Messages are now escaped, length-bounded, and asserted
  `isprintable()`.

- **The census loader silently dropped records.** 17 of 7,319 records share a
  line with the next one, and splitting on lines lost *both* halves each time —
  quietly, since a mangled record simply fails to parse. That removed 34 files
  from every comparison. Records are now matched by shape rather than by line,
  and a mismatch between records present and records parsed is reported.

- **Files that stopped early were classified as over-reporting.** Excess
  outranked truncation, so 172 truncated reads were filed under the wrong
  heading. Truncation now wins, because excess is only evidence about the
  reader when the reader reached the end.


- **A rule reference, in the app** (`MLOX_RULES.md`). The rule maker can now
  write every rule the format has, which made "what should I write?" the harder
  question. The reference answers it, organised by what you are trying to *say*
  rather than by rule name, and it opens from the **Help** menu and from a
  **Rule guide** button inside the rule maker — offline, rendered by the same
  viewer as the Read me.

  It is written from scratch rather than copied. The conventions it describes
  are the community's and are credited as such, with a link to the guideline
  page as the authority; the wording and the examples are ours. A test checks
  that every rule kind the rule maker offers is actually described in it, so the
  two cannot drift apart, and another checks the document is in the build
  manifest — a Help entry that works from a checkout and opens empty in the
  release is otherwise found only by a user.

- **Declare your own grass.** Holding back what `openmw.cfg` already calls
  groundcover only helps a mod that is already installed *and* declared. One you
  just added is in neither place, so you can say so once: a `groundcover=X.esp`
  line in a subset file, `groundcover = [...]` in the TOML form, `--groundcover`
  on the command line, or the **Declare as groundcover** field in Options. The
  plugin is then kept out of `content=` and written as `groundcover=` in both
  the patched cfg and the emitted customizations (as an `append` entry, which is
  how the Configurator writes a groundcover line).

  Its `data=` folder is still inserted normally, deliberately: OpenMW has to be
  able to find the file for the groundcover line to mean anything.

- **Conflicts can now be seen, not just listed** (`mlox_subset/viz/`). Four
  self-contained HTML views, generated from data the tool already had and
  opened from the Conflicts and field-diff windows:
  - a **conflict map** plotting every colliding record onto the world grid,
    with a breakdown of *what* is being edited (terrain shape, NPC navigation,
    the cell record) rather than a bare count. This is an **alternative** to
    the cell map, not a change to it: that map answers which mods *touch* a
    cell, this one answers which mods *edit the land record and path grid* in
    it and how those edits conflict. The two are independent views built by
    separate buttons -- neither links to or depends on the other, so neither
    can be left broken by the other failing to generate. The cell map's own
    SVG is untouched.
  - a **terrain height difference**, decoding every contributing plugin's
    `VHGT` to absolute heights and showing the **chain of edits** that produced
    the cell: each step diffs a plugin against the one immediately before it in
    load order, red where that step raised the ground and blue where it lowered
    it, opening on the winner's own step. Landscape records do not merge -- the
    last plugin to touch one replaces it wholesale -- so "what did this plugin
    change" is a question about its predecessor, not about the eventual winner. This is the view that makes landscape diffs
    honest: heights are stored as cumulative deltas, so moving **one** vertex
    changes every byte after it and two nearly-identical cells look completely
    different in the raw field.
  - a **path-grid graph**, drawing the navigation mesh with edges the winner
    added in green and removed in red. A plugin that *only* removes edges has
    likely rebuilt its path grid by accident, which strands NPCs and which
    nothing else in the toolchain reports; the page says so explicitly.
  - a **3D terrain surface** you can rotate, with each plugin's version
    switchable in place. Since grown a good deal: see **Fixed** for the vertical
    scale and **Changed** for the relief shading, contours and controls.

  All four are dependency-free and work offline -- no CDN, no external script,
  matching the cell map's existing guarantee. The 3D view is hand-rolled on a
  2D canvas for exactly that reason. The severity palette (green/yellow/red)
  follows `merged_lands` (MIT), which established it for TES3 land conflicts.
- **A Help button** on the main window, offering the Quick start and the Read me
  rendered as readable pages with a contents sidebar. Handing the `.md` to the
  operating system was rejected -- what opens is whatever happens to be
  associated with `.md` on that machine -- so the Markdown is rendered by the
  app itself, offline, with no CDN and no JavaScript. Both documents are now
  bundled into the frozen build.
- **A "Format reference" view** beside a record diff: every subrecord the record
  type is documented to contain, whether the game requires it, how wide it is,
  and the named fields inside the struct ones. A diff says what changed; this
  says what it was. Built from the new TES3 schema (see below), and offered only
  for record types the reference actually covers, so the button can never open
  an empty window.
- **Fields in the diff window are labelled in the file format's own terms** where
  the correspondence is known: `vertex_heights` is also "VHGT - Height Data
  (struct, 4,232 bytes, optional)". Where it is *not* known the field is left
  unlabelled -- a confidently wrong label would send a reader looking in the
  wrong place, which is worse than no label at all.
- **MWSE and MW-Enhanced script functions now disassemble.** 360 opcodes the base
  game has no equivalent for, taken from `customfunctions.dat` (a data file in
  MWEdit's own format, installed by the MWSE updater -- not MWSE source; see
  `CREDITS.md`). Calls to them are marked in the listing, because a script using
  one will not run at all without that runtime installed.
- **Fixed: `XFileWriteFloat` disassembled one operand short.** MWEdit's function
  table omits its filename parameter, so every call to it desynchronised the rest
  of the instruction stream. Corrected against UESP's documented syntax and the
  three sibling `XFileWrite*` functions, which all take the filename first.
- **"Tidy old HTML views"** on the main window (default on, remembered between
  runs). Every generated map and terrain view is timestamped so successive ones
  can be compared, which means the folder accumulates -- and on a big load order
  a single conflict map is megabytes. On close, the newest three of each kind are
  kept and older ones removed, along with their sidecar data folders. Two
  guarantees: it only ever deletes files matching one of this tool's own
  filename stems **plus** a timestamp -- never a blanket `*.html` sweep, and
  specifically never an un-timestamped page you saved yourself -- and with the
  box unchecked nothing is removed at all.
- **A generated timestamp on the cell map**, matching the other HTML views. An
  hour-old map that looks identical to a fresh one is a real trap when several
  are sitting in the same folder.
- **Scrolling and resizable panes on the conflict map**, matching the cell map:
  the map and the worst-cells list each scroll independently and can be dragged
  taller.

### Changed

- **The conflict map is banded like the cell map** -- each of the first five
  counts gets its own colour, larger counts group in fives, and the legend has
  one swatch per band instead of sampling a gradient. Same reasoning as the cell
  map: a linear ramp normalised against the worst cell rendered one, two and
  three conflicting records as three near-identical greens, and those are the
  counts that decide whether a cell is worth opening. The two maps are read one
  after the other, so banding them differently would have been the worse trap.

  **Scaled to the true maximum now, not the 95th percentile.** The percentile
  clamp existed to stop one forty-conflict cell flattening every ordinary cell
  to green -- a real problem for a continuous ramp, and one banding solves
  outright, since an outlier lands in the open-ended top band and costs the
  lower bands nothing. The legend now describes the range the map actually has.

  The page's client-side redraw (focusing one plugin) **looks a count up in a
  table** rather than re-implementing the ramp in JavaScript. The duplicated
  curve was the likeliest thing to drift between the focused and unfocused
  views; a lookup cannot drift, because there is only one copy of the
  arithmetic. `severity`, `severity_stops`, `legend_stops` and
  `saturation_point` were removed with it -- all four had become dead code held
  alive only by their own tests.

- **The customizations TOML now uses `insertBlock`.** A run of consecutive
  custom plugins is one block on one anchor instead of one `insert` per plugin
  chained on its predecessor. Besides being far shorter to read, it removes a
  real failure: anchors are matched by *substring* and more than one match makes
  momw-configurator abandon the cfg it was building, so chaining gave every
  plugin its own chance to collide. Inserting `Wares.esp` into a list that ships
  `Better Wares.esp` used to abort the whole rebuild; it now applies cleanly.
  The anchor is checked for uniqueness before it is written, falling back to
  anchoring `before` the following line when the preceding one is ambiguous.
- **Disabling your own mod no longer writes a `removeContent`/`removeData`
  block.** The Configurator rebuilds the cfg from the curated list plus these
  customizations, so a mod we simply stop inserting is already gone -- the block
  did nothing except clutter a file people hand-edit. Removals are now emitted
  only for what the curated list owns, for plugins and data paths alike. Without
  a `plugin-order.yml` there is no curated list to consult, so the old
  presence-based behaviour stays as the fallback.

- **The cell map's colours are now banded**: 1, 2, 3, 4 and 5 mods per cell each
  get their own colour, then 6-10, 11-15, and so on. The distinctions that matter
  are crowded at the bottom of the range -- one, two and three mods in a cell are
  different situations, while 23 and 24 are not -- and a continuous ramp
  normalised against the busiest cell on a big map rendered all of the low counts
  as the same dark blue. The legend lists every band, so it is now the map's key
  rather than a sample of a gradient. Above 16 bands the top one becomes
  open-ended (`76+`): a ramp is only readable while its steps are.

- **Wider colour ranges on both maps.** The severity ramp went from three stops
  to five: with only green → yellow → red the whole middle of a busy map
  collapsed into one narrow yellow band, so cells with genuinely different
  conflict counts looked identical. Coverage now has its own seven-stop ramp
  (slate → blue → periwinkle → violet → amber), deliberately *not* green-to-red,
  because coverage is not badness -- ten mods touching a cell is normal in a big
  load order -- and it should not be mistaken for the conflict map at a glance.
  Both legends are now generated from the same ramp the map draws with, so they
  cannot drift apart, and the conflict map's client-side recolouring is handed
  the stop table as data instead of re-implementing the curve in JavaScript.
- **Out-of-range cells are reported, not silently dropped.** One corrupt grid
  coordinate would stretch the map to millions of pixels, so filtering them is
  right -- but the page now says how many were dropped rather than quietly
  rendering an incomplete map.

### Fixed

- **Grass mods are no longer inserted as `content=`.** Reported by a user as
  "it toggles on ALL mods, including mods flagged as grass mods". A folder scan
  cannot tell a grass mod from any other mod -- it walks a directory and takes
  every plugin it finds -- so a shared mods folder put grass plugins into the
  subset alongside genuinely new content, and they were then written as
  `content=` in both the patched openmw.cfg and the emitted TOML. Since the
  plugin was already on a `groundcover=` line, it ended up declared **twice**:
  OpenMW loaded the grass through the groundcover system *and* spawned every
  blade as a real object, which is precisely the cost groundcover exists to
  avoid, arriving silently.

  Any plugin the cfg declares on a `groundcover=` line is now held out of
  `content=`, and the run says which plugins it held back and why. The decision
  is made on **what your cfg declares**, never on the filename -- a `*grass*`
  or `*groundcover*` pattern would wrongly hold back a plugin like
  `deleted_groundcover.omwaddon`, which is ordinary content (a real case, in
  the project's own sample cfg). The `groundcover=` lines themselves are never
  touched, and the mod's `data=` path is still written, since OpenMW has to be
  able to find the file for its groundcover line to work.

- **The 3D terrain view is shaded like a relief map.** A greyscale *hillshade*
  carries the shape and a hypsometric *tint* (green valleys through tan and rock
  to pale summits) is composited over it at 55%, adjustable from 40% to 75% or
  off. Keeping them as two layers is the point: flat-filling one blended colour
  per face -- what it did before -- fuses "which way does this face" with "how
  high is it" into a single number, so neither could be read on its own.

  **Shaded per pixel, not per face.** The mesh is 32x32 after sampling, so a
  face is tens of pixels across and its edges were plainly visible as facets on
  what should be a smooth hillside. Interpolating the normal and the height
  across each triangle costs a lookup and a few multiplies per pixel and removes
  them. The light is fixed to the *terrain* rather than the camera, so turning
  the model turns it under the light like a real object -- a light pinned to the
  viewer keeps every slope equally lit however you rotate it, which is the one
  thing hillshade exists to prevent.

  **Contour lines**, derived in the same pass from the height already
  interpolated at each pixel. The interval is a round number (1, 2 or 5 times a
  power of ten) chosen to put about a dozen lines on the cell, and it is named in
  the readout -- a contour without a stated interval measures nothing. Line width
  is divided by the local slope so lines stay a constant width on screen instead
  of fattening on flat ground, and where the spacing would fall below three
  line-widths the lines are dropped rather than allowed to merge into a dark
  smear over exactly the cliffs the hillshade is describing. Paper maps drop
  them for the same reason.

  **Isometric and Top down buttons.** Neither viewpoint can be hit by dragging:
  true isometric needs a pitch of arcsin(tan 30 degrees) so all three axes
  foreshorten equally, and top-down needs an exact right angle.

  **Every setting is exposed** on a control panel: shading mode, hillshade
  on/off, light count, scale count, sun azimuth and altitude, tint palette and
  opacity, contours, and vertical exaggeration. Nothing was replaced to add
  them — the defaults reproduce the view exactly as it stood, including the
  light direction, which was a hard-coded vector and is now the same direction
  written in degrees. (Exposing it revealed that the vector's own comment
  claimed north-west while the vector was south-west; the vector was right.)
  Controls that cannot act grey out rather than disappearing, and **Reset**
  restores all of them from one block of defaults rather than a hand-maintained
  list.

  **Multidirectional lighting** (3 or 6 lights) spreads lights evenly around the
  compass at the chosen altitude, weighted toward the primary azimuth. One light
  leaves whole faces in flat black where nothing can be read; several fill those
  shadows without flattening the relief. Total brightness is unchanged — the
  weights sum to one and all lights share an altitude, so flat ground is lit
  identically at one light or six and only the shadow side moves.

  **Multiscale shading** blends slopes measured over three window widths, because
  a narrow window describes texture and a wide one describes landform, and one
  radius has to choose.

  **Three tint palettes**: hypsometric (default), a rainbow in the order Turbo
  popularised, and greyscale. The rainbow is written from our own stops rather
  than lifted from Google's table — the ordering is the useful part and is a
  fact about rainbows. It resolves small differences far better than a
  sequential ramp, which is both why it is offered and why it is not the
  default: it implies boundaries the ground does not have.

  **Both shading modes are switchable** from the panel: relief by default, the
  original flat-shaded facets one click away. A faceted surface makes the mesh
  itself visible, and "where are the vertices" is occasionally the question
  being asked. The *geometry* is identical in both — the vertical scale is a
  correctness matter and not a style, so switching shading can never bring back
  the distortion the flat view originally shipped with. A test asserts exactly
  that. Contours work in either mode.

  The tint ramp is handed to the page as a lookup table rather than
  re-implemented in JavaScript, the same decision as the conflict map's band
  table and for the same reason. 256 samples, which puts the largest step
  between neighbours at two units per channel -- at 64 it was seven, which shows
  as a band on the ramp's steepest segment.

- **The 3D terrain view was drawn 55x too steep.** Reported as "from the top it
  looks correct, but from the side the slope is way too extreme" -- which is the
  signature of a *normalised* height axis, because looking straight down hides
  the vertical entirely and only an oblique view shows it.

  Heights are in world units, and 65 vertices span a cell's 8,192 units, so
  there are 128 world units between adjacent vertices. The renderer plotted x
  and y as vertex *indices* but scaled height to a constant 110 units --
  `((z-lo)/span)*110` -- so every cell was drawn the same height on screen
  whatever its actual relief, on a footprint 32 units wide. A cell with 512
  units of relief should stand 2 units tall; it stood 110. **The exaggeration
  was worse the flatter the terrain**, which is exactly why gentle hills read as
  cliffs.

  Height is now divided by the real world-unit spacing (times the sampling
  stride, which widens the horizontal step and would otherwise reintroduce the
  bug at 2x). A 45-degree slope in the world is now a 45-degree slope on screen,
  which is the property the tests assert. A **Vertical** control offers 2x, 5x,
  10x and 25x for reading genuinely flat terrain, defaulting to 1x -- and the
  height readout says so whenever the view is exaggerated, so a distorted shape
  can never be mistaken for a true one.

- **The emitted TOML could abort the Configurator outright, and did.** Found in
  a real generated file: 389 insert entries over 2,229 lines, and one of its
  anchors fatal.

  - **`data=` inserts never got the `insertBlock` treatment.** That change
    landed for `content=` only, so the data half still wrote one
    `[[Customizations.insert]]` per path -- 372 of them in the reported file,
    152 sharing a single anchor. They are now one block per contiguous run: 389
    entries become 39.

  - **A data anchor is now checked for uniqueness, and widened when it is not.**
    The Configurator matches anchors with `strings.Contains` against whole
    lines and treats more than one match as **fatal for the entire run** -- it
    returns a nil cfg, so nothing is applied. `_anchor_is_unique` existed but
    was wired only into the content path. In the reported file
    `...\UvirithsLegacy\Data Files` was chosen as an anchor while
    `...\UvirithsLegacy\Data Files\Addons` was also a real line, so the anchor
    matched twice.

    Rather than give up on an ambiguous anchor, the emitter now *widens* it to
    the whole cfg line, which is very often unique where the bare value was
    not, because the line carries delimiters the value lacks:
    `data="...\Data Files"` is not a substring of `data="...\Data Files\Addons"`
    -- the closing quote ends it. The same widening fixes a long-standing
    content-side case: `Wares.esp` is a substring of `Better Wares.esp`, but
    `content=Wares.esp` is not a substring of `content=Better Wares.esp`. That
    ambiguity previously forced a fallback to the other neighbour; now the
    natural anchor survives. Where widening cannot help -- an unquoted path
    that is a prefix of another has no delimiter to widen to -- the other
    neighbour is still tried, and only then is the ambiguous anchor emitted
    with its warning, because a rebuild that stops and says which line was
    ambiguous beats a cfg quietly missing mods.

  - **The `after` reversal had to go with it.** N separate inserts sharing one
    `after` anchor each land immediately after that same line, so they come out
    reversed and were deliberately *written* reversed to compensate. A block is
    placed as a unit and keeps its own order, so carrying the reversal across
    would have silently inverted every run anchored that way. Both directions
    are now pinned against `simulate_configurator_apply`.

  The equivalence harness had modelled two forms -- chaining each insert on the
  previous one, and a single block -- but never the third the data emitter
  actually used, N inserts on one *fixed* anchor. That is why none of this was
  caught. It is covered now, along with the data emitter end to end.

- **The app could refuse to start over drag and drop.** `HAVE_DND` recorded
  whether the *Python* package `tkinterdnd2` imports, and every drop-target
  registration then assumed the **tkdnd Tcl package was loaded into the
  interpreter** -- which is a different fact, and only true when the root
  window was built with `TkinterDnD.Tk()`. Where they disagree (a
  half-installed tkdnd, a frozen build that shipped one side without the
  other), the first path field raised `TclError` during construction and took
  the whole window build with it. No window, and a traceback pointing at a text
  entry rather than at the missing package.

  Registration is now guarded and probes the interpreter rather than the
  import, so a missing tkdnd costs exactly what it should: no drag and drop,
  Browse buttons instead, and the banner that says so now tells the truth
  instead of reporting on the import.

  Found by running the new Tk suite on a real desktop for the first time --
  the suite built a plain root, every test errored during setup, and the test
  bug and the product bug were the same mistake.

- **Six defects found by auditing the release's own new code.** Each is fixed
  with a test, and each test was verified by re-introducing the defect and
  confirming a red run. Five of the six were in code written for 3.1, which is
  the point of auditing new work rather than only reviewing it.

  - **A rule could silently lose a plugin.** mlox reads a line beginning with
    whitespace as message text, so a name typed into the rule maker with a
    leading space vanished from the rule -- and the rule still loaded, still
    looked right, and simply did not apply to that plugin. Verified against the
    real loader before fixing. Names are now stripped.
  - **`table()` could silently drop rows** (`viz/html.py`). It paired rows with
    per-row attributes using `zip`, which stops at the shorter list, and the
    list that runs short is the attributes -- so a caller one attribute shy lost
    a *table row*. On the conflict map that means losing a conflict. The
    project's blanket `B905` exemption claims every `zip()` was reviewed
    individually; this one had not been. Now padded.
  - **A conflict record with `plugins` as a string produced ten proposals about
    single letters** (`rules/derive.py`). A bare string is iterable, so
    `"A.esp"` where `["A.esp"]` was meant was iterated by character. That module
    exists to keep guesses from being presented as facts, so confident nonsense
    is the one output it must never produce.
  - **`@@Section`** when the field is labelled `@section:` and the guidelines
    write sections as `@Name`, so typing the `@` -- the natural thing to do --
    doubled it.
  - **An out-of-range highlight priority rendered no mark at all**, silently,
    which is not what asking for one means. Now refused.
  - **A dead `_REF` regex** in `rules/authoring.py`, defined and never used.

  A sweep for the same shapes elsewhere found no others: no `TODO`/`FIXME`
  markers, no unreferenced private names, and the docs renderer emits no
  `<script` under any input tried against it.

### Internal

- **A headless Tk smoke job in CI** (`tests/test_gui_smoke.py`, run under
  `xvfb`). The GUI is excluded from the hermetic suite and from mypy because it
  needs Tk, which left it with no automated coverage at all -- and the last two
  defects to reach a user were both there. The job builds the real application
  and checks that every button exists and is bound, that no two widgets are
  gridded into the same cell, and that each window opens with content in it. It
  fails if the tests *skip*, since a skip would otherwise pass green having
  checked nothing.

  **Extended** to the things a checklist reads past: every shipped Help document
  renders to a real page, every log theme applies and repaints, the backups
  window replaces itself instead of stacking, a format reference opens for six
  record types rather than the one, and settings survive a save and load --
  each string field, each checkbox, and the rule list in order.

  That last one generalises: a test compares the keys `_gather_settings` writes
  against the keys `_load_settings` reads, because an option saved but never
  loaded is written on every exit and discarded on every start, with nothing
  erroring and the file looking correct. The manual `SMOKE_TEST.md` pass is now
  scoped to what a test cannot judge -- whether the output is *right*, and
  whether the screen is readable.

  **16 tests to 42, and run for real:** 42 passed, 0 skipped on Windows 11 /
  Python 3.14.5 / pytest 9.1.1. The expected count is recorded in
  `SMOKE_TEST.md` because a suite that quietly collects 38 instead of 42 has
  lost four checks and still reports green.

  Zero *skipped* is the harder half of that. A skip means the check did not run,
  and the first version of this suite hid its most important test behind one:
  the case covering the drag-and-drop bug below needed a second Tk root, which
  the environment declined to give it, so the check silently did not happen.
  It is now simulated on the existing root instead and cannot skip. CI fails the
  job on any skip for the same reason.
- **76 annotation-only imports moved under `TYPE_CHECKING`**, and ruff's `TC`
  rules enabled so they stay there. Safe without exception because every module
  uses PEP 563 string annotations and nothing introspects them at runtime;
  verified by importing all 53 modules in a fresh interpreter, not just by a
  green suite.
- **`lint_plugins` decomposed** (201 lines -> 88, over seven small checkers).
  Byte-identical output confirmed against a probe that trips every lint branch
  at once.
- **CI now tests 3.10, 3.11, 3.12 and 3.13** -- every version
  `requires-python` promises, rather than just the two ends.

- **A native TES3 record schema** (`mlox_subset/tes3fields/schema.py`, generated
  by `tools/gen_tes3_schema.py`): 46 record types, 313 subrecords, 62 with parsed
  struct layouts, built from UESP's format-page tables. 56 of those layouts
  declare a plain byte count, and all 56 agree with the sum of their parsed
  members -- which is how four parser defects were found and fixed, each of which
  had silently dropped a field.
- **`_build_controls` split by panel** (435 lines -> 34, largest piece 99). It was
  a flat wall of widget construction, which is the shape of code that makes
  adding one button a nervous edit; the Help button that landed in the same
  release is a two-line change because of it.

- **The cell map generator moved out of the engine** into
  `mlox_subset/viz/cellmap.py`, with its CSS and JS in
  `mlox_subset/viz/cellmap_js.py` as plain constants (`CODE_REVIEW.md` §29). It
  was 216 lines of HTML/CSS/JS in one f-string sitting in the middle of the sort
  engine, with every brace doubled -- `REMAINING_WORK.md` flagged it as
  effectively uneditable, and that is why it could not grow the features asked
  of it. It is now ten small functions that each return a fragment, and the
  client assets need no escaping at all. `mlox_subset_sort.py` keeps the public
  name as a delegation, so nothing that imported it had to change.
  `tests/test_viz_pages.py` adds 85 tests over the result, five of which were
  verified by injecting the real defect and confirming a red test.

## 3.0

Two headline changes.

**1. The engine is now a real package.** What was a single monolithic script is
split into `mlox_subset/` — 7 subpackages, 33 modules — and
`mlox_subset_sort.py` is now the engine and CLI rather than a monolith: it
imports from those packages like any other caller. The GUI and the tests import
from them directly too, so each name has exactly one import path. What changed
is that the pieces are now separable enough to test and read individually.

That is not bookkeeping — **it is what surfaced the correctness fixes in this
release.** Splitting the code exposed API that existed but was never called,
and twice now a dead accessor has turned out to be a real bug hiding in plain
sight: the `[SIZE]`/`[DESC]` fix below is `PluginFileIndex.usable`, written and
then never wired up, which meant rules asserted matches for plugins that were
not on disk. The suite grew to **984 tests** over the course of the split
(374 at the midpoint), alongside a differential baseline that pins 41
behavioural observations against a real 687-plugin load order — which is what
made a refactor this size possible without taking sorting behaviour on trust.

**2. The theme picker now themes the entire GUI, live** — window, buttons,
frames, tabs, lists and entries, not just the log panel, and switching
re-themes every open window immediately.

Also here: script `bytecode` and `variables` fields are decoded in the
field-diff window, and the PEP-conformance and blind-except passes.

### Added

- **Translation marking, complete.** `mlox_subset/i18n.py` provides gettext
  lookup, plural handling and language auto-detection (`$MLOX_LANG`
  overrides), and **every user-facing string is marked** — buttons, labels,
  tooltips, dialogs, and the report/status messages that were built with
  f-strings, converted to named-placeholder form
  (`_("Loaded %(count)d files") % {"count": n}`) with `ngettext` for counted
  messages. `locale/mlox_subset_sort.pot` is the extracted English template
  (**393 messages**), regenerated by the new **`tools/make_pot.py`**:
  standard-library only, so it works on Windows without GNU `xgettext`, and
  AST-based, so a `_()` inside a docstring is correctly not extracted. Pure
  data output (plugin names, `content=` lines, section banners) is
  deliberately unmarked. The pipeline was proven end-to-end against a
  compiled test catalogue — translation, plural selection, English fallback.
  No language ships yet; with no catalogue installed every lookup returns the
  English source unchanged.
- **`tools/check_placeholders.py`** — the checker that makes the placeholder
  form safe to use at scale: for every marked string formatted with `% {...}`
  it verifies the `%(key)s` names against the dict's keys in both directions
  (a mistyped key is otherwise a *runtime* `KeyError`, which the suite cannot
  reach in the GUI), and rejects positional `%s` in marked strings outright
  because translators reorder words. Proven against deliberately broken
  inputs in `tests/test_i18n_placeholders.py`; runs in CI and the gate list.
- **`-v/--verbose` on the CLI**, wiring up the levelled-logging foundation
  that shipped with the package split: diagnostics about the run (an
  unparseable rule file, a failed CSV write) now go to **stderr via
  `logging`** — WARNING and worse by default, `-v` adds progress, `-vv`
  per-item detail — while the report you asked for stays on stdout, pipeable
  and clean. In the GUI the same diagnostics land in the log panel as before.
- **Coverage floor.** The measured full-suite coverage (54%, branch) is now
  enforced with `fail_under = 52`, set slightly below the honest number so it
  ratchets upward instead of blocking the next change.
- **PEP 639 licence metadata**: `license = "MIT"` as an SPDX expression plus
  `license-files`, replacing the deprecated table form; pinned by a new
  standards test alongside the existing PEP conformance suite.
- **mypy now gates the entire codebase** — all **38** files, up from 28.
  Every module, including both legacy scripts, is fully annotated (the engine
  went from 19/200 typed arguments to 200/200; the GUI from 2/84 to 84/84) and
  PEP 257 clean, so every `D`/`ANN` exemption and `ignore_errors` override was
  deleted rather than relaxed. Turning the checker on found real bugs, not just
  style: two functions declared `list[str | Path]` parameters that — `list`
  being invariant — could not accept the `list[str]` their callers actually
  build (`PluginFileIndex`, `check_predicates`; both now `Sequence`); the
  tes3cmd worker could dereference a `None` staging path; and seven
  hand-written annotations were flatly contradicted by the code they described.
  The window mixins now declare the attributes they expect from their host
  `App` in a `TYPE_CHECKING` block, so that coupling is checked instead of
  implicit.
- **The packaging metadata is now exercised, not just declared.** CI runs
  `python -m build`, and the suite asserts that every package and module the
  metadata declares exists on disk — a `[build-system]`/`[project]` pair can
  be syntactically valid and still unbuildable. Building it for the first time
  confirmed the wheel is sound, and that the `setuptools>=77` floor PEP 639
  requires is genuinely newer than some distros ship (it fails loudly, which
  is correct). A companion check asserts every name in `__all__` resolves.
- **18 new built-in themes** (23 total): Monokai Pro, Tokyo Night, Night Owl,
  Nord, Shades of Purple, GitHub Dark, Catppuccin Mocha, Ayu Dark, Cobalt2,
  SynthWave '84, Winter is Coming, Material Dark, Bluloco Dark, Palenight,
  Poimandres, Noctis, Panda and City Lights — each with the scheme's published
  syntax palette *and* hand-filled chrome (window/button/field colours) from
  its own UI slots, so the whole app re-themes, not just the log panel.
  Dracula and the One Dark palette (as "Atom One Dark") were already built in
  and are unchanged.
- **Landscape and path-grid fields are decoded in the field-diff window.**
  Previously only a script's `bytecode` and `variables` were; everything else
  stored as binary showed as base64, which is actively misleading in a diff —
  two landscape cells differing by one vertex produce *entirely* different
  base64, so a one-vertex nudge read as "completely different". Now the five
  `LAND` grids (vertex heights, normals, colours, texture indices, world map)
  render one terrain row per line, with heights reconstructed to absolute
  world units, and a path grid's `connections` renders as a per-point
  adjacency list. Two of these are only meaningful beside a sibling field —
  heights need their `offset`, edges need their `points` — so the whole record
  is passed to the decoder, the same way `bytecode` uses the record's `text`.
  Validated against real plugins and real tes3conv output, which is what caught
  a decoding bug worth naming: tes3conv **prefixes `connections` with a uint32
  count** (100% of 717 path grids checked), and left in place that prefix
  shifts every edge by one slot — silently attributing each path point its
  *neighbour's* connections.
- **`theme_template.json`** — a commented, import-ready starting point for
  custom themes, sitting next to the app. Covers the 9 required fields, the 7
  optional syntax-token roles (and what each falls back to), and the optional
  `"chrome"` override object with all 11 window-colour keys. Imported as-is it
  reproduces the default palette exactly, so it doubles as a reference for
  what the built-in "Dark (default)" theme actually is.

### Changed

- **The engine was split into the `mlox_subset/` package.** Seven subpackages
  by concern — `rules/` (mlox pattern matching, parser, expression front-end),
  `sort/` (graph primitives + load-order engine), `configurator/` (openmw.cfg
  read/simulate/emit), `plugins/` (file location + header metadata), `net/`
  (rule and curated-order downloads), `mwscript/` (compiled-script decoding),
  `gui/` (Tk theming, widgets and the window mixins),
  `momw.py`/`versions.py`/`tracing.py`/`i18n.py`. `mlox_subset_sort.py` briefly
  re-exported the moved names so the split could land without touching every
  call site; that shim was removed before release, and the GUI, CLI and tests
  now import from the packages directly. The package was held to a
  stricter standard than the legacy scripts while they caught up (full typing,
  PEP 257 docstrings, no silent excepts); by the end of 3.0 that standard
  applies to every shipped file, enforced in `pyproject.toml`.
- **The two largest functions were decomposed.** `compute_plan` went from
  **644 lines to 105** and `build_and_sort` from **476 to 119**, each split
  into helpers named for the pipeline stages their own comments already
  marked. Bodies moved verbatim, so report and trace output are unchanged —
  and the 41-observation differential baseline stayed green throughout, which
  is the only reason a refactor of the two functions whose output *is* the
  product was attempted at all.
- **The GUI was split too** (the second half of the same job):
  `mlox_subset_sort_gui.py` went from ~5,600 lines to ~3,200, with the
  separable pieces moved **verbatim** into a new `mlox_subset/gui/`
  subpackage — `theme.py` (chrome palette, theme parsing, the live restyle
  walk, the JSON/HTML highlighters), `widgets.py` (tooltip, queue writer,
  path field, drag-reorder listbox, typeahead), and the two window groups as
  mixins the `App` class inherits: `t3.py` (`Tes3cmdMixin`, the tes3cmd
  front-end) and `conflicts.py` (`ConflictWindowsMixin`, the record/resource
  conflict windows and field diff). `app_base_dir()` moved with them. Every
  name is re-imported by the main module, so behaviour, the smoke-test
  instructions and the build config are unchanged. The exemptions the moved
  code arrived with were then paid off in this same release (see the mypy
  entry above), so `mlox_subset/gui/` now meets the package standard rather
  than the legacy one. Its runtime verification is the SMOKE_TEST.md §2/§5/§6
  run, since there is no Tk in the hermetic suite.
- **The theme picker now themes the whole GUI, live** (task #43). What was a
  log-panel-only syntax setting now drives an app-wide chrome palette:
  window, buttons, frames, tabs, entries, lists, scrollbars, tooltips,
  pane-divider grips and console-style panes all follow the selected theme,
  and switching it re-themes every open window immediately (a ttk.Style
  re-configure for the ~160 ttk widgets, plus a recursive walk over the live
  widget tree for the plain-tk remainder). Built-in presets carry hand-tuned
  chrome from each scheme's published UI palette; base16 imports take theirs
  from the base00–base04 UI slots; native-JSON imports may supply an optional
  `"chrome"` object — and any chrome colour not given explicitly is derived
  from the theme's background (lightening dark themes, darkening light ones),
  so every existing imported theme keeps working unchanged. With the default
  theme the GUI looks as it always has.

### Fixed

- **Open field-diff viewers now follow theme switches fully.** The runtime
  re-apply walk recoloured an open diff window's chrome and text background
  but left its syntax-token tags (json_key/json_string/html_tag/...) on the
  previous theme's colours; the walk now re-runs `style_json_syntax_tags`
  with the new theme on any Text widget that has those tags. Found in smoke
  testing, which is exactly what it is for.
- **ttk widget colours now apply in the compiled .exe.** The GUI picks a
  colour-capable ttk base theme (`clam`, falling back to `alt`/`default`/
  `classic`) rather than silently swallowing a failed `theme_use("clam")` and
  landing on a Windows-native theme that ignores colour options — the reason a
  frozen build previously left the main window's buttons/frames/tabs on the
  default grey while the log panel (a plain-tk widget) themed correctly. The
  active base theme is now traced, as is a build stamp (version + whether the
  run is frozen + build time) — a stale `.exe` is otherwise indistinguishable
  from a code bug, and the Log panel now shows the running build on every start.

- **A latent `NameError` in `compute_plan`.** `for line, is_new, _ in
  data_result:` bound `_` as a function-local, which would have made every
  `_()` lookup earlier in that same function raise. Harmless until the gettext
  marker was introduced, and caught immediately by ruff's `F823` when it was —
  the throwaway targets are now named (`_anchor`, `_is_new`).
- **`[SIZE]` and `[DESC]` rules no longer assert a match for plugins that are
  not on disk.** These predicates fall back to "assume true" when the plugin
  cannot be inspected -- mlox does the same, deliberately, to avoid raising a
  warning it cannot substantiate. But mlox gates that on having *no data
  directory at all* (`self.datadir is None` in its `ruleParser`), whereas this
  tool applied it whenever an individual plugin was not found.

  With readable mod folders and one missing plugin, every `[SIZE]`/`[DESC]`
  rule about it therefore claimed a size or description match for a file known
  not to exist -- which can fire a `[Conflict]` or `[Requires]` warning that
  is simply wrong. Now distinguished via `PluginFileIndex.usable`.

  The bug survived earlier audits because `testdata/` has no `data=`
  directories, so the index is unusable there and the old behaviour was
  correct. It is now covered by four tests, including the readable-directory
  case that reproduces it.

### Documentation

- The field-diff tree has a tooltip: double-clicking is now discoverable, and
  it says which fields are decoded (`bytecode`, `variables`) rather than shown
  raw.
- The **Check Conflicts** tooltip mentions script disassembly, which it
  predated.

### Internal

- **CI and coverage measurement** (`CODE_REVIEW.md` §9.3, §8.5).
  `.github/workflows/ci.yml` runs the whole gate list — ruff, black, mypy,
  `check_undefined`, `make_pot --check`, pytest — on Python 3.10 and 3.13, and
  installs `zstandard` so the 3 bytecode tests actually run instead of skipping.
  Coverage is configured in `pyproject.toml` with branch tracking, the GUI
  omitted (it cannot be imported without Tk, so counting it would report a
  meaningless ~0%), and deliberately **no `fail_under` yet** — the floor should
  be set from the first real CI run rather than guessed. `--cov` is passed by CI
  rather than baked into `addopts`, so a plain local `pytest` still needs
  nothing but pytest.
- **`CODE_REVIEW.md` is now labelled as the running log it always was**, with a
  §16 reconciling its older "roadmap" and "recommendations" sections against
  what actually shipped. Notably: §15's list of oversized functions predates the
  split and misses the largest one (`compute_plan`, 545 lines). The re-export
  shim §15 recommends deleting was deleted before release (§23), which is why
  3.0 makes no `core.<name>` compatibility promise to be held to later.
- **`BLE001` (blind-except) is now enforced.** All 68 `except Exception` sites
  were reviewed individually: 28 narrowed to their provable raise-set
  (`ValueError` for TOML/JSON decode, `(OSError, ValueError)` for the
  documented `fetch_url_bytes` contract, `(OSError, SubprocessError)` for
  checked `subprocess.run`, `tk.TclError` for stale-widget calls), and 40 kept
  broad with a `# noqa: BLE001` and a stated reason — untrusted rule-file
  input, worker-thread top levels that report tracebacks into the log, optional
  third-party imports, and deliberate "unexpected" backstops. Rationale in
  `CODE_REVIEW.md` §13. No behavioural change intended; the full test suite and
  the differential baseline re-ran green after each narrowing.
- **PEP conformance is verified rather than asserted.** `tests/test_standards.py`
  mechanically checks 15 PEPs that define a standard for this codebase -- PEP 8
  (now including **naming** and **import order**, which were never enforced),
  257, 484/526, 563, 585/604, 3120, 263, 3131, 328, 440, 621, 561, 594, 632,
  394, 518/517, 508, 420. Enabling the missing ruff rulesets found 18 issues.
- **`[project]` metadata and a `[build-system]` table** now exist (PEP 621 /
  518), with `py.typed` (PEP 561) so a consuming type checker stops silently
  ignoring the package's annotations.
- **mypy is clean and now gates.** It found 22 errors when first enabled --
  every one in a hand-written annotation, including a `Sequence` the body
  concatenated and an int-typed dict holding floats.
- **PEP 20**: the one mechanically checkable line ("Errors should never pass
  silently. Unless explicitly silenced.") is now a test. It found two silent
  `except: pass` handlers with no stated reason.

- **Compiled scripts are now readable in the field-diff window.** Double-click
  a `bytecode` field and you get a disassembly -- named instructions with their
  operands -- instead of a wall of base64. Previously any script edit at all
  looked like a total rewrite, because the whole blob changed.

  The listing is deliberately honest about its limits. Morrowind's compiler
  stores expressions (the `x == 1` in an `if`) as semi-textual data rather than
  opcodes, so no table-driven disassembler can decode a whole script. Anything
  not recognised is printed verbatim as offset/hex/ASCII and the walker
  resynchronises on the next known opcode, so it never desyncs and never
  invents an instruction. A `; decoded: N%` header tells you how much of the
  stream was accounted for.

  When the record carries its source text, that is used to suppress false
  positives: an opcode value occurring by chance inside expression data is only
  decoded if the script really calls that function. On the test corpus this
  took false positives from 52 to zero.

- **The `variables` field is decoded too.** It carries the script's local
  variable names under the same base64+zstd wrapping, so the diff previously
  showed only that the blob differed, never *which* locals changed. It now
  lists them in declaration order.

  Worth noting how close this came to shipping wrong: the field has a 4-byte
  length prefix, and a first pass that split straight on NUL produced a junk
  leading "name" on all 118 corpus scripts -- one variable too many, every
  time. Checking against the record headers caught it. With the prefix
  stripped, the body length matches both the prefix value and
  `header.variables_length`, and the name count matches
  `num_shorts + num_longs + num_floats`, across all 120 records tested.

- **Opcode table rebuilt from MIT-licensed sources only.** The table is
  generated from MWEdit's `Functions.dat` by `tools/gen_opcodes.py`, plus one
  compiler-internal opcode (`_SetReference`) measured from real scripts rather
  than copied from anyone. An earlier draft merged in MWSE's `OpCodes.h`, which
  is **GPLv2** -- that data has been removed. See `CODE_REVIEW.md` §10.

- **Attribution corrections.** `tes3cmd` is by John Moonsugar (MIT), not Paul
  Halliday; `tes3lint`'s evil-GMST table now carries its MIT notice inline; and
  abot's `missing_pathgrids` / `cell_conflicts` are credited properly, with the
  note that only their ideas were used, never their code.

## 2.3

- **Type-to-jump in every list.** Click a list (plugin order, data paths,
  rule files, tes3cmd plugins, backups) and just start typing a name:
  prefix match first, substring fallback, press one letter repeatedly to
  cycle its matches, Backspace edits, Esc clears. The panel title shows
  what you've typed; the buffer resets after a short pause.
- **Configurator dry-run preview on every TOML export.** The emitted
  customizations are applied to a simulated fresh curated cfg using a
  faithful re-implementation of momw-configurator's own apply logic
  (cfg/custom.go: substring matching, insert/replace/remove/append order,
  same-anchor stacking quirks, ambiguity aborts) and the result is verified
  against the sorted order — `VERIFIED` in green when the round trip is
  exact, a red `MISMATCH`/`PREVIEW ABORTED` with details when it isn't. What
  the Configurator will do to your cfg is now known before it runs.
- **Save Check.** Pick an `.omwsave` and every content file it depends on
  (the SAVE record's DEPE list) is verified against the sorted, enabled
  order — OpenMW refuses to load a save with missing plugins, so this warns
  before an export orphans a character.
- **Backups window.** Lists every backup this tool, tes3cmd and the
  Configurator leave behind (`.preclean.bak`, `.masterfix.bak`, `name~1.esp`,
  timestamped `.bak-*` / `.backup.*`) across the data folders, with
  restore-over-original and delete.
- **Rule maker hardening** (checked against the mlox rule guidelines). A rule
  that lists the same plugin twice is now rejected — ordering a plugin relative
  to itself is a self-cycle mlox would discard. And when a new `[Order]` rule
  contradicts the frozen curated (MOMW) order, the maker warns before writing
  that mlox will discard those orderings (it never reorders the curated list),
  so you don't get a silently-ineffective rule. Engine cycle handling was
  re-verified against the guidelines: conflicting orderings are discarded (no
  hang), user-file rules win over base rules, and the curated order is never
  broken. The comment field now hints at the `(Ref: ...)` citation convention.
- **Rule maker.** A "New Rule..." button on the rule-files panel writes mlox
  rules without knowing the syntax: pick `[Order]` / `[NearStart]` /
  `[NearEnd]`, build the plugin list by grabbing the selected rows from the
  plugin panel (their displayed order becomes the rule order), typing names
  (wildcards and `<VER>` allowed, validated with the same regex the parser
  uses — a rule that writes is a rule that loads), add an optional `;;`
  comment, watch the live preview, append. Rules go to a personal file
  (mlox_base/mlox_user are refused — "Update Rules..." would overwrite them)
  that's auto-added LAST in the rules list, so your rules win conflicts.
  This is how rules for modern mods get made; contribute good ones upstream.
- **Configurable download sources.** A "Sources..." button on the rule-files
  panel opens a dialog for pointing the two updaters at a fork or mirror if
  upstream moves: an mlox-rules URL template (must contain `{name}`, filled
  with `mlox_base.txt`/`mlox_user.txt`) and a plugin-order.yml URL. Both
  persist in settings, blank means the built-in defaults, and
  `$MLOX_RULES_URL_TEMPLATE` / `$MLOX_PLUGIN_ORDER_URL` still work as env
  overrides. Downloads are validated before anything is written regardless of
  source. (The plugin-order.yml default now points at the current upstream
  location, `.../momw/momw/data_seeds/data/plugin-order.yml`, with the GitLab
  API raw endpoint as a fallback.)
- **Tooltips stay on screen.** A tooltip on a right-edge or bottom-edge widget
  (common when the window is maximized) is now clamped to the screen — it
  slides left to fit and flips above the widget when there's no room below,
  instead of being cut off past the edge.
- **Two-row action layout.** The action buttons are split across two compact,
  left-aligned rows — primary + read-only analysis on top (with the status
  label trailing), tools below — so the growing toolset doesn't crowd into one
  long row.
- **Update plugin-order.yml button** (next to its path field). Downloads the
  current MOMW plugin-order.yml, trying the website then the site's GitLab
  repo (`$MLOX_PLUGIN_ORDER_URL` overrides for mirrors). The download must
  parse as plugin-order data with hundreds of entries before a single byte
  is written — an error page or moved URL can never clobber the file — and
  the old copy is kept as a timestamped .bak.
- **Update Rules button.** Downloads the current `mlox_base.txt` /
  `mlox_user.txt` from the actively maintained rules repo
  (github.com/DanaePlays/mlox-rules — the same source plox uses and mlox
  1.1+ auto-updates from) over the matching configured files, keeping
  timestamped backups; shows each file's age first. Personal rules files
  with other names are never touched.
- **New lint checks:** `[TWIN]` — an active `.omwaddon`/`.esp` whose
  `.omwscripts` sibling sits in the same folder but isn't in the load order
  (or vice versa), which silently disables a mod's Lua half; `[EXP-DEP]` —
  scripts calling Tribunal/Bloodmoon-only functions in a plugin that doesn't
  master the expansion (tes3lint's !TB-FUN/!BM-FUN, comment-aware).
- **Watchdogs:** `[STALE]` warns when `delta-merged.omwaddon` /
  `deleted_groundcover.omwaddon` / `S3LightFixes.esp` is older than active
  plugins (the merge no longer reflects the load order — re-run the
  Configurator); the GUI warns on Export when openmw.cfg changed on disk
  since the Sort.
- **`--lint` CLI flag** for the same checks the GUI Lint button runs.
- **Unconstrained mods keep YOUR declared order.** The subset was being
  alphabetized on input, so mods that no rule or dependency constrains landed
  at the end A→Z instead of in the order written in your subset file /
  customizations TOML (or scan order). Declaration order is now preserved
  (de-duped, not sorted). *(user feedback)*
- **Multi-line mlox expressions parse correctly.** An indented line inside a
  [Note]/[Conflict]/[Requires] body is only message text when no bracket is
  open — mlox conditions like `[ALL a.esp ⏎ [NOT b.esp] ⏎ c.esm]` continue
  across indented lines, and treating the continuations as message text
  truncated the condition (e.g. the Uvirith's Legacy "Children of Morrowind"
  note fired for people without Children of Morrowind, with the lost
  condition text leaking into the message). *(user feedback)*
- **removeContent / removeData etc. are emitted one entry per line**, matching
  the style of MOMW's own documentation examples instead of an unreadable
  single line. *(user feedback)*
- **Every emitted insert is annotated with its REAL constraint.** The
  `after=` in the generated TOML is the mod's chained position (documented
  Configurator semantics, kept deliberately — see below), but a comment above
  each insert now says *why* the sort put it there: `# constraint: must load
  after 'X'` (header master or mlox rule), `must load before 'X'`, an mlox
  NearStart/NearEnd hint, or `# no ordering constraint -- positional only`.
  The generated file reads like dependency documentation without betting the
  load order on the Configurator's undocumented same-anchor stacking
  behaviour. *(user feedback)*
- **Ambiguity warnings, verified against momw-configurator's source.** Its
  `cfg/custom.go` matches `after`/`before`/`source` values with
  `strings.Contains` against whole cfg lines and hard-errors on multiple
  matches — so a filename nested inside another (`Incantation.omwscripts`
  inside `content=Incantation.omwscripts.esp` — a real pair on a real list)
  breaks the run. Worse, `remove*` entries use the same substring match with
  NO multi-match error: every matching line is deleted **silently**
  (path-like values instead match exactly / by suffix). The emitted TOML is
  now checked both ways and collisions are flagged with the exact lines.
  Warn-only; output unchanged. Also confirmed from source while in there:
  same-anchor `before=` inserts stack in file order but same-anchor `after=`
  inserts stack in REVERSE file order — undocumented either way, which is
  why this tool keeps explicit chained anchors.
- **Cell map: "Focus on mod" filter** (the good idea in cell_conflicts.pl).
  A dropdown above the map — customs first, starred — dims every cell the
  chosen mod doesn't touch, filters both cell lists to match (combined with
  the existing text filter), and summarizes its footprint: how many
  exterior/interior cells it touches and which other mods share those cells,
  ranked by overlap. One click answers "what does this mod actually edit,
  and who else is in those cells?".
- **Lint: native tes3lint-style checks.** A Lint button runs ports of the
  worthwhile tes3lint / missing_pathgrids.pl diagnostics directly on the
  plugin binaries (VFS-aware, no perl needed): `[EVLGMST]` — the 72 evil
  GMSTs, flagged only when name AND value match tes3lint's table so
  deliberate changes aren't accused (cross-validated: tes3cmd clean removes
  exactly the ones we flag); `[FOGBUG]` — interior cells with AMBI fog
  density 0.0 (black-void bug), exact port including the behave-like-exterior
  exemption; `[NO PATHGRID]` — new interior cells with no pathgrid anywhere
  in the load order (improves on the reference script, which missed grids
  supplied by later plugins); `[HEADER]` — customs with a blank
  author/description. Vanilla masters and merged/multipatch artifacts are
  skipped, like the reference scripts do.

## 2.2

Sort-engine correctness, a conflict-detection fix, faster repeat scans, and UI
polish.

**Load-order engine (correctness)**

The subset sorter now places your custom mods properly instead of leaving many
of them stuck or dumped at the end:

- **Customs already in the cfg are no longer frozen in place.** The frozen chain
  is now built from the **curated list only** — custom mods already present in
  `openmw.cfg` are bridged over, so they can actually be re-sorted against the
  curated list and against each other. (Previously each custom was locked between
  its current neighbors and mlox rules couldn't move it.)
- **Header-master dependencies are honored.** Each custom plugin's TES3 header
  masters are read (from the cfg's data= folders *and* the data paths being
  added this run), and it's forced to load **after** every master it declares.
  Applied only to customs; the curated list is never touched.
- **Customs interleave — position comes from the whole graph, both directions.**
  A custom's place in the list is resolved from **all** of its graph neighbors,
  transitively:
  - *"After" anchors (preferred):* a custom lands right after the latest-loading
    non-master thing it must load after — a curated plugin (header master *or*
    mlox rule) or **another custom**, resolved through custom→custom chains. A
    patch of a patch of a custom mod follows its whole chain to the right spot.
  - *"Before" anchors:* a custom with no dependency anchor but an mlox rule
    saying it loads *before* something is placed just before its earliest such
    successor. Previously these customs kept their end-of-list position, and
    when the frozen chain reached their curated successor, the sort stalled
    there and dumped **every** pending custom in one alphabetical block — the
    "big block" bug.
  - Circular derivations are detected and skipped (a "before B" custom can't in
    turn be used to anchor B), `.esm` predecessors give no position signal
    (they'd cluster everything at the front), and truly standalone customs plus
    `.omwscripts` go to the end, where the Configurator would append them too.
- **Rule files parse correctly: plugin names with spaces no longer shatter.**
  `[Order]`/`[NearStart]`/`[NearEnd]` blocks were split on *all* whitespace
  instead of per line, so any rule mentioning a multi-word plugin name
  (`Friends & Frens - TR.ESP`, `Beautiful cities of Morrowind.ESP`, most of
  mlox_base…) dissolved into junk tokens — the rule silently didn't apply, and
  stray wildcard fragments matched plugins they were never about, creating
  bogus edges (and a bogus mid-list cluster). Rules with spaced names are now
  enforced, and edges only come from rules that really name the plugin.
- **Full parser audit against the reference implementations** (mlox-master's
  `ruleParser.py` and plox's `parser.rs`), fixing every divergence found:
  - *Rule headers only start at the beginning of a line* (both references
    require this). Previously a message line mentioning e.g. "[Order]"
    mid-sentence started a phantom rule block and corrupted block boundaries.
  - *[NearStart]/[NearEnd] are position hints, not ordering chains.* They were
    being chained like [Order], inventing edges between unrelated plugins
    (mlox_base's [NearEnd] alone linked Merged Objects.esp → Mashed Lists.esp
    → …). They now pull matching customs toward the start/end, edges permitting
    — real mlox semantics.
  - *mlox_user.txt rules now beat mlox_base.txt in conflicts*, matching mlox
    (which reads user rules first so they win). Precedence was inverted.
  - *Order-block lines are parsed like the references:* a name runs to a
    recognized plugin extension with trailing junk dropped (mlox), multiple
    extension-delimited names per line are accepted (plox), and conditional
    `[DESC …]`/`[SIZE …]` qualifier lines inside Order blocks are bridged over
    as not-installed phantoms exactly like mlox treats them.
  - *[Conflict]/[Requires]/[Note] message lines are identified by indentation*
    (mlox's actual rule) instead of by content-sniffing, which had turned
    thousands of indented mlox_base message lines that mention a plugin name
    into phantom logic operands — source of false conflict/note warnings.
    Header-line comments now also appear in the warning text.
  - *Smaller parity fixes:* UTF-8 BOM no longer hides a header on a file's
    first line; `[DESC]` predicates with brackets inside their `/regex/`
    tokenize correctly; `[SIZE]` accepts OpenMW plugin extensions.
- **ESM-first.** Master-type plugins (`.esm`/`.omwgame`) now tie-break before
  ordinary plugins, so a custom master with no rule floats up into the master
  block instead of sinking to the bottom.
- **The sort is deterministic run to run.** Rule-pattern expansion and the
  anchor resolver used to iterate Python sets, whose order is randomized per
  process — so a fresh app launch could produce a different (equally valid)
  order than the last one. All graph iteration now happens in a fixed order;
  the same inputs give the same output every time.

- **tes3cmd clean is now VFS-safe (staged).** tes3cmd only understands one
  flat "Data Files" directory, so on an OpenMW multi-folder setup it couldn't
  see a plugin's masters — cleaning without masters gives wrong results. Clean
  now stages each plugin into a private Morrowind-shaped folder (minimal
  Morrowind.ini + Data Files with the plugin's masters, hardlinked when
  possible and cached across runs) and runs tes3cmd there; the cleaned result
  is copied back only on success, with a one-time `.preclean.bak` of the
  original. Plugins whose masters can't be found are skipped outright, files
  are cleaned masters-before-dependents in load order, and a "MOMW
  needs-cleaning" button queues exactly the plugins plugin-order.yml flags.
  Verified against real tes3cmd: duplicate-of-master records removed, new
  records kept, original untouched. **multipatch was removed** — it needs the
  entire load order in one flat directory (unfakeable for a multi-GB setup),
  and OpenMW/MOMW users get merged leveled lists from delta-plugin instead.
- **Master-size resync is now done in-app, not by tes3cmd.** tes3cmd's
  `header --synchronize` assumes one flat "Data Files" directory; on an OpenMW
  multi-folder layout it can't find the masters and writes **empty sizes**
  into the plugin header (observed corrupting real plugins). The tes3cmd
  window's resync now resolves each master across ALL data folders and
  rewrites only the 8-byte size fields (one-time `.masterfix.bak` per file,
  idempotent, verified byte-exact). Headers zeroed by a bad tes3cmd sync are
  flagged by the master check and repaired by the same resync. Also, a
  manually-entered tes3cmd path now wins outright or errors — it never
  silently falls back to another copy found on the system.
- **tes3cmd frontend.** A `tes3cmd` button next to Resource Conflicts opens a
  frontend for tes3cmd (auto-detected; the compiled tes3cmd.exe from the MOMW
  Tools Pack is preferred, the pure-perl script works when perl is installed):
  clean plugins, `header --synchronize` to fix `[MASTER SIZE]` notes, view
  headers, or build multipatch.esp. "My mods (last sort)" fills the file list
  with your customs located across the data folders (including pending ones);
  output streams to the log; modifying commands confirm first and rely on
  tes3cmd's own backups. Morrowind.esm, Tribunal.esm and Bloodmoon.esm are
  **never cleaned** — even a careful GMST-preserving clean rewrites bytes
  other content depends on and causes in-game failures — the frontend skips
  them with a warning rather than trusting tes3cmd's own name check.
- **Plugins with master problems are flagged in the load-order panel.** Rows
  whose plugin has a missing or mis-ordered master render in purple (red
  already means "touched by this sort", gold means "yours" on the cell map),
  matching the MASTER CHECK section in the log.
- **Missing-master check on every sort.** Each active plugin's TES3 header
  masters (MAST/DATA subrecords) are verified against the final load order:
  `[MISSING MASTER]` (red) when a required master is absent — distinguishing
  "installed but not in the load order" from "not found in any data folder,
  the game will fail to load"; `[MASTER ORDER]` (red) when a master loads
  after its dependent; and tes3cmd-style `[MASTER SIZE]` notes (orange) when
  the installed master's size differs from what the plugin was built against.
  Custom mods are checked before the cfg is written, and warnings carry the
  mod's origin (scan / customizations.toml) so it's clear which is yours.
- **Conflict / Cell Map / Resource scans now see your custom mods BEFORE the
  cfg is written.** All three scans (and the CLI equivalents) searched only the
  data= folders already in openmw.cfg, so pending custom mods — the very thing
  being sorted — were invisible to them ("0 involve your custom mods") until
  after export. They now search the cfg's folders plus every pending custom
  data path from the scan/customizations TOML, so you can check conflicts and
  adjust the order before committing anything.

**Fixes**

- **Pathgrid conflicts no longer collapse into one bogus record.** Interior-cell
  pathgrids all carry grid `(0, 0)`, so (under tes3conv) every interior's pathgrid
  from every plugin was being merged into a single fake `PathGrid (0, 0)` conflict
  spanning hundreds of plugins. Pathgrids are now keyed by their cell (name for
  interiors, coords for exteriors), so only plugins editing the *same* cell's
  pathgrid are flagged. (Cached scan sidecars are versioned and rebuild
  automatically for this fix.)

**Performance**

- **Scan caching for fast repeats.** The first Check Conflicts / Cell Map reads
  each plugin's JSON once and writes two tiny per-plugin sidecars in a single pass
  — `*.keys.json` (record ids, for conflict detection) and `*.cells.json` (cells
  touched, for the map) — so running both features reads each big JSON only once
  per run. Later scans read those few-KB files instead of re-parsing the multi-MB
  JSON, so **repeat Check Conflicts and Cell Map runs are near-instant**. Sidecars
  are mtime-invalidated per plugin; the on-click field diff still reads the full
  record, so accuracy is unchanged.

**UI**

- **Custom mods flagged in the field comparison.** Your custom mods are marked
  with a ★ in the Check Conflicts field-comparison column headers, and shown in
  orange (vs grey for curated-list plugins) in the double-click field popout, so
  it's obvious which side of a conflict is yours. The popout also gained a
  **Word wrap** toggle for long values.
- **App icon.** A vector program icon (`art/mlox_subset_sort_icon.svg`) plus a
  multi-size `.ico` for the built exe.

## 2.1

Performance and packaged-build (`.exe`) fixes, focused on big load orders and the
Windows one-file build.

- **tes3conv JSON reused within a run.** Conversions always spool to a stable
  `tes3conv_json` folder and are reused, so Check Conflicts followed by Cell Map no
  longer re-runs tes3conv; a plugin is only re-converted if it changed. "Keep
  tes3conv JSON dump" now only controls whether that folder is kept or removed on
  exit.
- **No more tes3conv console-window popups** in the windowed / auto-py-to-exe
  build — tes3conv is launched with `CREATE_NO_WINDOW`.
- **Embedded cell-map window now appears in the exe.** A console-suppression flag
  (`SW_HIDE`) was being inherited by the pywebview child's WebView2 window, so it
  spawned hidden — looking like a hang and leaking processes, then falling back to
  the browser. The viewer launch no longer hides its window.
- pywebview is the preferred in-app cell-map viewer; detection is a real import
  (reliable when frozen). A `cell_map_viewer.log` records the viewer's outcome, and
  `MLOX_MAP_VIEWER=pywebview|tkinterweb|browser` can force a viewer.
- README: PyInstaller/auto-py-to-exe steps for bundling pywebview
  (`--collect-all webview clr_loader pythonnet`, `--hidden-import clr
  webview.platforms.edgechromium`).

## 2.0

Added the inspection tools on top of the 1.0 sorter:

- **TES3 record-level conflict detection** (Check Conflicts) — flags records that
  two or more plugins define/override (last one wins), via a built-in binary
  parser or, if a `tes3conv` binary is available, tes3conv for exact record ids
  and **field-by-field diffs**.
- **Cell map** — a modmapper-style SVG heatmap of which mods touch which
  exterior/interior cells, with tabs and click-to-jump.
- **Data-path (VFS) resource conflicts** — same loose file provided by 2+ `data=`
  folders (later wins), like MO2's "Data" conflicts.
- Supporting work: exclude patterns for noisy mods, saved settings, disk-backed
  tes3conv (bounded memory on big lists), an in-app cell-map viewer, and
  frozen-`.exe` (PyInstaller / auto-py-to-exe) support.

## 1.0

First public release: the subset sorter. Sorts only your custom mods into an
existing `openmw.cfg` using mlox rules **without** reordering the curated
Modding-OpenMW.com list, and emits a corrected `momw-customizations.toml`.
Included the mlox-ported rule engine (wildcards/`<VER>`, order transitivity,
`[Conflict]/[Requires]/[Note]` + `[VER]/[SIZE]/[DESC]`), `plugin-order.yml`
curated-vs-custom awareness, a drag-and-drop GUI + full CLI, a mods-folder
scanner, row opt-out, and cross-platform (Windows/Linux/macOS) support.
