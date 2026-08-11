# Merged Lands

This is the single reference for the Merged Lands port: what was ported and what
was not, the fidelity audit that read the two sources side by side, the
comparison against the OpenMW fork, and the function-by-function coverage table.
It consolidates four documents that used to stand alone
(`MERGED_LANDS_PORT.md`, `MERGED_LANDS_FIDELITY_AUDIT.md`,
`MERGED_LANDS_FORK_COMPARISON.md`, `MERGED_LANDS_FUNCTIONS.md`) so there is one
place to read and one place to keep current.

Merged Lands is MIT (David Von Derau, 2022). Licence text vendored at
`License/MergedLands/LICENSE`; see `CREDITS.md`. The function coverage table is
generated -- see *Function-by-function coverage* below.

---
## Port coverage: what is ported, what is not

A file-by-file audit of `merged_lands-main/src` against `wraithguard/land/`,
written because "we ported it" had been asserted more than it had been checked.
Every row states where the behaviour lives here, or says plainly that it does
not exist yet.


## Coverage

| merged_lands source | Lines | Status | Where it lives here |
|---|---|---|---|
| `land/terrain_map.rs` | 127 | **ported** | `land/diff.py` — `LandData`, flat interleaved grids |
| `land/grid_access.rs` | 55 | **ported** | `land/diff.py` — `RelativeGrid.offset_of` and friends |
| `land/conversions.rs` | 58 | **ported** | `tes3fields/landscape.py` decoders, predating this work |
| `land/height_map.rs` | 186 | **ported** | `land/heights.py` — both directions, plus normals |
| `land/landscape_diff.rs` | 252 | **ported** | `land/diff.py` — `LandscapeDiff`, `diff_against_reference` |
| `land/textures.rs` | 324 | **ported** | `land/textures.py` — `KnownTextures`, translation, compaction |
| `merge/relative_to.rs` | 85 | **ported** | Implicit: Python ints subtract without a trait |
| `merge/relative_terrain_map.rs` | 205 | **mostly** | `land/diff.py` — see *Gaps* on `recompute_vertex_normals` |
| `merge/conflict.rs` | 127 | **ported** | `land/merge.py` — `average_delta`, `ConflictParams`, `Severity` |
| `merge/round_to.rs` | 37 | **ported** | Implicit: `int()` |
| `merge/merge_strategy.rs` | 150 | **ported** | `land/merge.py` — `merge_layer`, `DEFAULT_STRATEGY` |
| `merge/resolve_conflict_strategy.rs` | 63 | **ported** | `ConflictStrategy.RESOLVE` |
| `merge/overwrite_strategy.rs` | 46 | **ported** | `ConflictStrategy.OVERWRITE` |
| `merge/ignore_strategy.rs` | 46 | **ported** | `ConflictStrategy.IGNORE` |
| `repair/seam_detection.rs` | 338 | **ported** | `land/seams.py` — corners then edges |
| `io/save_to_plugin.rs` | 298 | **ported** | `land/emit.py` |
| `io/parsed_plugins.rs` | 350 | **replaced** | We read `tes3conv` JSON rather than parsing TES3 |
| `repair/cleaning.rs` | 176 | **ported** | `land/cleaning.py` + `compact_textures` |
| `io/meta_schema.rs` | 95 | **ported** | `land/meta.py` — `.mergedlands.toml` |
| `main.rs` (merge flow) | 808 | **ported** | `land/pipeline.py` — the six steps in order |
| `merge/cells.rs` | 100 | **ported** | `land/cells.py` — opt-in `--cells` (off by default), references never carried |
| `io/save_to_image.rs` | 359 | **ported** | `land/conflict_image.py` — `--conflicts-dir` |
| `repair/debugging.rs` | 79 | **ported** | `land/debug_colors.py` — `--add-debug-vertex-colors` |

**Every file is now ported.** The two entries marked *replaced* and *implicit*
are behaviour that exists here in a different shape, not behaviour that is
missing: plugin parsing goes through `tes3conv` rather than a TES3 reader of our
own, and Rust's `RelativeTo`/`RoundTo` traits are what Python integers already
do.

## The structural bug the audit found

The tool merged only *contested* cells — those more than one mod edited —
because those are the ones with anything to resolve. That is true of the diff
and merge steps and **false of seam repair**. Seams are shared between
*adjacent* cells, and a contested cell's neighbour may have been edited by only
one mod. Excluding that neighbour means its side of the shared border is never
reconciled and the tear survives the repair meant to remove it.

This is why Merged Lands merges every modified cell and cleans at the *end*
rather than filtering at the start. `land/pipeline.py` now does the same.

Measured on two overlapping Solstheim mods: 33 modified cells merged, seam
repair moved 202 vertices (13 at corners, widest gap 3,016 units), cleaning
dropped 26 redundant cells — and **5 single-mod cells were kept only because
seam repair had moved them**. Under the old shape those 5 were never candidates
at all, and their borders would have stayed torn.

## `CELL` merging: opt-in, off by default

Terrain is the whole job, so `Merged Lands.esp` carries `LAND` records only
unless you ask otherwise. **`CELL` merging is off by default** (`--cells`;
`include_cells=False`, and the GUI does not expose it). This follows the OpenMW
Merged Lands fork, which removed it outright (`cells.rs` deleted): the OpenMW
Merged Lands discussions landed on cell-attribute merging not being worth the
ownership it takes, and the value of the merge being in the land. We default to
that rather than reproduce the original's always-on behaviour, and keep the code
as an explicit opt-in instead of deleting it, for the rare setup that wants it.

When enabled, `--cells` emits `CELL` records alongside the terrain: region, water
height, map colour, name and flags — the things a mod reshaping land frequently
adjusts too (lowering water to match a dug channel, renaming a cell). The rest of
this section is why that opt-in is safe when it is used.

**References are never carried.** Merged Lands emits `references: default()` —
an empty list — and so does this. That is what makes it safe: reference lists
merge by `(mast_index, refr_index)` rather than being replaced wholesale, so a
`CELL` record with no references does not displace anything placed in the cell.
Verified on a real write: 7 `CELL` records emitted, every reference list empty.

Flags are unioned rather than overwritten — two mods that each set a different
bit both meant it.

**One divergence, which is a bug fix.** `merge_cells_into` reads:

```rust
if let Some(record) = new.region.as_ref() {
    new.region = Some(record.clone());
```

`new` on both sides — the field is cloned from itself, so a later plugin's
region, map colour, water height and atmosphere never apply. The same shape
appears for all four. We take the incoming value, which is the evident intent;
reproducing the typo would mean a merge tool that cannot merge four of the seven
fields it reads.

## Where this port goes further than the original

Not everything is a deficit. Four differences are deliberate improvements, each
driven by something measured:

**Averaging is refused on categorical layers.** Merged Lands lets a
`.mergedlands.toml` set `Resolve` on `texture_indices`, which averages index 3
and index 7 into index 5 — a third, unrelated texture. `merge_layer` raises
instead.

**New land is never averaged.** A cell the masters never had has no common
ancestor, so two landmass mods that both define it have authored two unrelated
landscapes rather than edited a shared one. Merged Lands treats every vertex as
contested and blends; we force last-wins. On a 1,033-plugin dump this is 10,212
of 10,633 contested cells, and it moved the "far from at least one mod's intent"
count from 2,091,861 to 1,434,606.

**An unencodable gradient is reported, not fatal.** `VHGT` deltas are signed
bytes, so adjacent vertices cannot differ by more than 1,016 world units. Merged
Lands asserts and aborts the run; `encode_vertex_heights` returns the clamped
vertices so the other cells still merge.

**Curvature weighting.** `ConflictStrategy.CURVATURE`, opt-in, weights an edit
by the structure it introduces rather than by how far it moves a vertex —
following Zhao, Jiang and Guo (2022) §2.3. A +500 bulk shift introduces 0.000
radians of structure; a −60 road cut introduces 0.297. Magnitude alone gives the
shift eight times the say.

## Where the code has moved on since this was written

The audit above is a statement about *behaviour*, and the behaviour is
unchanged. Two things about the implementation are not:

- **The slope limiter sorted its cells on every pass.** `sorted(cells)` sat
  inside the pass loop, so a 17,560-cell landmass was sorted 24 times to
  produce the same list 24 times. The order is there to make the result
  deterministic, and no pass adds or removes a cell, so it is now sorted once.
  No merged plugin changes as a result; this is speed only.

- **`emit.py` states each field's payload size once.** The empty-field branches
  used to spell out `bytes(3 * LAND_NUM_VERTS)` and friends alongside a
  `FIELD_SIZES` table that said the same thing and had no caller. They now read
  from the table. An empty field of the wrong length still encodes and still
  writes, so this is the kind of duplication that stays wrong quietly.

Neither is a divergence from Merged Lands: both are internal to `land/`.

---

## Fidelity audit against `src`

**Date:** 2026-08-04 · **Original:** `merged_lands-main/src` (Rust, MIT, David Von
Derau 2022; 27 files, ~4,400 lines) · **Port:** `wraithguard/land/` (Python, 18
files, ~6,680 lines).

**Question asked:** does the port actually reproduce the Rust source, or has "we
ported it" been asserted more than checked? This audit reads the two side by
side for the functions where the details decide the output, on top of the
project's own mechanical coverage gate.

## Verdict

**The port is faithful. No fidelity defect found.** Every numerically-critical
algorithm matches the Rust statement for statement, including the rounding and
integer-division corner cases that are the usual place a reimplementation drifts.
The divergences that exist are all documented, deliberate, and either bug fixes
or measured improvements — never accidental. The port-coverage and function-by-function claims (above and below)
hold up.

## What was checked, and how

### 1. Mechanical coverage — every Rust function is mapped

`tools/gen_merged_lands_table.py --check --src ../merged_lands-main/src` parses
the Rust source, looks each function up in a hand-written coverage map, and fails
if any function is unmapped or any map entry names no function.

    191 function(s), all accounted for   (exit 0)

`tests/test_merged_lands_coverage.py` runs the same check when the source tree is
present. So "all 191 functions covered" is enforced, not asserted.

### 2. Tests — the land suite is green

299 tests across `test_land_fidelity`, `test_land_diff`, `test_land_heights`,
`test_land_merge`, `test_land_emit`, `test_land_native`, `test_land_landmass`,
`test_land_service`, `test_merged_lands_coverage`, `test_survey_landscape` — all
pass. `test_land_fidelity` in particular pins the port against Merged Lands'
numbers on real data.

### 3. Line-by-line reads of the fidelity-critical routines

Each of these was read in both languages and confirmed to compute the same thing:

**Height encode/decode** — `land/height_map.rs` vs `land/heights.py`.
`encode_vertex_heights` reproduces the VHGT scheme exactly: `offset =
stored[0][0]`, column 0 carries the row-to-row difference
(`stored[y][0]-stored[y-1][0]`), every other column the along-row difference
(`stored[y][x]-stored[y][x-1]`), `data[0][0]=0`, gradients clamped to a signed
byte. `decode_heights_from_deltas` accumulates the same way and resets each row
to column 0 — matching Rust's `height = grid[y][0]` reset. Scale factor 8 applied
identically. The offset's `f32`→`i32` truncation matches because the offset is
integer-valued.

**Vertex normals** — `calculate_vertex_normals_map` vs
`vertex_normals_from_heights`. Same cross product (`nx = -(east-here)·step`,
`ny = -(north-here)·step`, `nz = step²`, `step = 128/8`), same far-edge vertex
reuse (`fix_coords` ↔ `fx=x-1 if x==64`), same normalisation
(`length/127`, cast to signed byte).

**Conflict averaging & severity** — `merge/conflict.rs::classify_conflict` vs
`land/merge.py::average_delta`. Magnitude weight, the `^1.5` bias, the
renormalise, the blend, the signed `min` used for the threshold, and
`min(max(0.3·smaller, 10), 64)` all match. The Major/Minor boundary
(`|smaller − blended| ≥ threshold`) matches. **`round_to` checked specifically:**
Rust's `RoundTo` is `self as i32`/`as i8`, which truncates toward zero — so
Python's `int(blended)` is correct, despite the Rust method being *named* "round".

**Cell merge** — `merge/cells.rs` vs `land/cells.py`. Flag union (record and
data flags), id take-if-nonempty, and references-always-empty all match. The
`merge_cell_into` **self-clone bug is real and correctly fixed**: Rust lines
34–52 read `new.region`/`new.map_color`/`new.water_height`/`new.atmosphere_data`
(the accumulator) and assign them back to themselves, so an incoming plugin's
values never apply; the port reads `incoming[...]` instead. Reproducing the typo
would be a merge tool that cannot merge four of the seven cell fields it reads.

**Seam repair** — `repair/seam_detection.rs` vs `land/seams.py`. Corners are
repaired before edges in both, and both guard the edge pass against an unequal
corner (Rust `assert!(index != 0 && index != 64)` ↔ Python raising if a corner
is still unequal). Integer averaging matches — and the port's `mean()` explicitly
truncates toward zero (`-(-total//count)` for negatives) rather than using
Python's floor `//`, precisely to match Rust's `/` over Morrowind's mostly-negative
(underwater) terrain. Normals are re-masked to the moved heights afterward, as in
Rust.

**Texture translation** — `land/textures.rs` vs `land/textures.py`. The VTEX/LTEX
off-by-one is faithful: `vtex_of(i)=i+1`, `ltex_of(v)=v−1`, and VTEX `0`
("nothing painted") passes through unremapped — matching Rust's `IndexLTEX =
IndexVTEX+1` and `try_remapped_index` returning the default index unchanged.

### 4. The documented divergences are all present and all deliberate

Each is a place the port intentionally does *not* copy the Rust, for a stated and
usually measured reason — verified present in the code:

- **Categorical layers refuse averaging.** `merge_layer` *raises* if `RESOLVE` is
  asked for textures ("index 3 and index 7 give index 5, a third unrelated
  texture"); Merged Lands would silently average them.
- **New land is last-wins, not blended.** A cell the masters never had has no
  common ancestor, so two mods that both define it authored unrelated
  landscapes; the port forces last-wins where Merged Lands blends every vertex.
- **An unencodable gradient is reported, not fatal.** `encode_vertex_heights`
  clamps and records rather than asserting and aborting the whole run.
- **Curvature weighting** (`ConflictStrategy.CURVATURE`, opt-in) — an addition,
  weighting an edit by the structure it introduces (Zhao/Jiang/Guo 2022 §2.3).
- **The cell self-clone bug fix** described above.

## Minor observations (not defects)

- The fidelity gate is conditional on the Rust tree being checked out beside the
  toolkit; `test_merged_lands_coverage` skips when `merged_lands-main/src` is
  absent. That's the right call for a hermetic suite, but it means the
  completeness guarantee only bites in an environment that has the source. Worth
  knowing when reading CI output that shows it skipped.
- Precision: the Rust conflict math runs in `f32`, the port in Python `float`
  (`float64`). Results can differ only at an exact truncation boundary, and the
  fidelity tests pin the real-data outputs, so this is immaterial — noted only
  for completeness.

## Fidelity audit: bottom line

This is one of the more carefully-done ports I've audited: the completeness claim
is machine-checked against the source, the numeric edge cases (truncation
direction, off-by-one, the VHGT reset, the self-clone typo) are each handled with
an explanatory comment, and every divergence is a decision with a reason attached
rather than an accident. The port faithfully reproduces `merged_lands-main/src`.

---

## The OpenMW fork: what to take

**Compared:** `OpenMWMergedLands-main/src` (the OpenMW fork) against
`merged_lands-main/src` (the original our port follows) and against
`wraithguard/land/`.

**Method:** file-level diff, then a code-only diff (doc comments stripped) of
every land/merge/repair source, to separate real behaviour from Rust
modernisation noise.

## Fork comparison: bottom line

The fork is a **modernisation + OpenMW-integration pass, not an algorithm-fix
pass.** ~95% of its diff is Rust idiom: clippy-pedantic casts
(`x as f32` → `f32::from`/`try_from`), `match` over if/else chains, stable
features (`default()` → `Default::default()`), digit separators, field renames
(`minor_threshold_pct` → `pct`, *same values* 0.3/10/64), `hashbrown` → std.
None of it changes the merged output, and none of it maps to anything our
Python port needs to do differently.

**Our port is already faithful to the merge algorithm, and in two places ahead
of the fork.** There is no critical work hiding here.

## The genuinely behavioural changes, and the call on each

| Fork change | Real? | Our port | Action |
|---|---|---|---|
| `i64` accumulators in corner-seam averaging | overflow guard | Python `int` is arbitrary-precision — cannot overflow | **none** |
| `f32_to_i8_saturating` / `is_finite()` guards in height encoding | NaN/inf robustness | heights are integer stored-units; NaN/inf can't arise | **none** (low value) |
| `has_difference` via `!=` instead of `average()` | equivalent (`average()` returns `None` iff equal) | already exact comparison | **none** |
| Dropped `CELL` records (`cells.rs` deleted) | feature *removal* | off by default like the fork; `land/cells.py` kept as an opt-in `--cells` | **follow the default; keep the escape hatch** |
| `fallback_texture_index` replaces the panic on an unremappable index | behavioural | we **pass through and report** (`translate_indices`), a documented, arguably-better choice | **your call — see below** |
| OpenMW app-config `merged_lands.toml` with `ignore_plugins` | new feature | we already skip plugins with no `LAND`/`LTEX` | optional convenience |
| Reads `openmw.cfg` `content=` order, outputs `.omwaddon` | OpenMW IO | we read `openmw.cfg` via the configurator and emit our own output | already handled our way |

## The one real design difference: unresolvable texture indices

When a merged cell paints with a texture index that no `LTEX` record defines
(usually a missing master):

- **Original:** panics (`remapped_index().expect(...)`).
- **Fork:** substitutes `fallback_texture_index()` — the smallest valid index —
  so the plugin is always valid but silently paints a *different* texture.
- **Our port:** passes the index through unchanged and records it as `unknown`,
  so the problem is named rather than hidden. `compact_textures` documents this:
  "substituting zero would silently repaint that terrain… so it is passed
  through and named."

Neither is strictly correct. Ours is more honest (it reports the fault); the
fork's guarantees a structurally valid plugin. **Resolved:** we now take the
fork's fallback **by default** and *still report it* -- `compact_textures`
substitutes the smallest valid painted texture, and the emit says how many
indices it substituted. Opting out (`substitute_unknown_textures=False`, a CLI
choice) restores the honest pass-through, also reported. A GUI-driven run gets a
plugin that loads without a hidden dangling index; a CLI run that would rather
see the dangle can ask for it. This was the only
item here worth writing code for, and it is additive.

## `.mergedlands.toml` schema (relevant to the next task)

`io/meta_schema.rs` is **unchanged** between original and fork (one cosmetic
`default()` edit). So the sidecar schema our task #4 must emit is the known one:
per-layer `[height_map]` / `[vertex_colors]` / `[texture_indices]` /
`[world_map_data]` tables carrying `conflict_strategy`
(`Overwrite`/`Ignore`/`Auto`/`Resolve`/`Curvature`) and `included`
(`true`/`false`), plus a top-level `meta_type` (`Auto`/`Patch`/`MergedLands`).
Task #4 can proceed against this directly.

## Recommendation

Close the "refine the port from the fork" task as **analysed — nothing critical
to port.** Optionally implement the texture-fallback emit mode (additive, small,
with a test). Otherwise the higher-value work is task #4 (`.mergedlands.toml`
generation, schema now confirmed) and the GUI-wiring queue.

---

## Second cross-check against the OpenMW fork (3.1.3)

The fork comparison above was a static file diff. This is a second pass, prompted
by the fork's own development thread (community bug reports of "black/weird
squares near water" and an "i8 addition overflow" panic) and a re-read of the
fork at its 1.0 state. Our carried `OpenMWMergedLands-main` is byte-identical to
that 1.0 tree — the Cargo version still reads 0.2.0, but every source file
matches — so this reads the current fork, not a snapshot.

Three concrete things came out of it, two of them defects we shared and have
now fixed:

- **Zero authored normals were inherited, lighting vertices black.** Both the
  original and our port, for a vertex whose height did not move, copied the
  reference cell's authored normal back over the recomputed one — unconditionally.
  A reference normal of `(0, 0, 0)` is missing data, not a lighting choice, and
  it is common in coastal and underwater cells; the engine lights a flat normal
  black. **The fork fixed this** (its `recompute_vertex_normals` keeps the
  recomputed normal where the existing one is zero, pinned by
  `recompute_vertex_normals_does_not_preserve_zero_existing_values`). We now do
  the same in `pipeline.resolve_normals`. This is a strong candidate for the
  "black squares near water" the fork's testers saw, which they had attributed to
  seam averaging.

- **An out-of-range i8/u8 aborted the merge.** The fork hit an `i8 addition
  overflow` panic on large load orders and fixed it by making `RelativeTo::add`
  saturate (`clamp` then `try_from`) rather than truncate-cast like the original.
  Our arithmetic is arbitrary-precision Python and never overflows mid-merge, but
  our *packers* (`pack_world_map`, `pack_vertex_normals`, `pack_vertex_colors`)
  fed `struct.pack("b", …)` / `bytes()`, which raise on an out-of-range value —
  and `struct.error` is not caught in `service._build_records`, so one such
  vertex would abort the whole run. The packers now saturate to the byte the
  format stores, matching the fork's behaviour and keeping the cell.

- **The default conflict strategy was a genuine design fork — and we adopted
  the fork's.** The original (and our port up to 3.1.2) defaulted contested
  vertices to `Resolve` — blend the two edits. The OpenMW fork changed the
  default so that `Auto` maps to `Overwrite` (`load_order_auto_strategy`): the
  later plugin wins the entries it actually changed, and blending happens only
  when a `.mergedlands.toml` asks for `Resolve` explicitly. Its README states
  this outright, and the thread frames the blend-everywhere default as "too
  aggressive," causing stretching. **In 3.1.3 `DEFAULT_STRATEGY` is now
  `Overwrite` for every layer**, matching the fork and re-positioning the tool
  as a *seam resolver* rather than a *conflict blender*. This changes merged
  terrain output: where two mods both moved a vertex, the later one now wins it
  instead of a magnitude-weighted average. Nothing was removed — the blend, the
  curvature weighting and the minor/major severity split all still run when a
  sidecar selects `Resolve` or `Curvature`, and both are one click away in the
  `.mergedlands.toml` editor. It is opt-in now rather than automatic.

Everything else checked out. The conflict-averaging math (magnitude weight raised
to 1.5, renormalised, threshold `clamp(0.3·min, 10, 64)`) is identical in both
Rust trees and in `merge.average_delta`. The fork's seam repair is the original's
midpoint averaging unchanged (its larger file is added tests); our `seams.py`
goes further with feathering, corner anchoring and `find_tears`. The fork removed
`CELL` merging entirely as bug-prone; ours is opt-in (`--cells`), off by default,
and already carries the self-clone and empty-reference fixes that caused the
duplication the fork's testers saw. And **neither Rust tree regenerates the world
map from heights** — our derive-when-absent-or-zero WNAM fix, which is what
actually resolved the brown/black squares here, is ours alone.

---

## Function-by-function coverage

Every `fn` in `merged_lands-main/src` — **all 191 of them**, named individually
— with where its behaviour lives here.

Written after a file-level audit proved insufficient: it showed every file
"ported" while five faults that would have shipped visibly broken terrain sat
inside functions nobody had read. Then rewritten again, because the first
version of *this* table grouped related functions onto one row and reported the
row count as the function count — so `land/` was labelled 37 functions when it
has 64, and forty functions were covered only by a heading that never named
them. A table that claims completeness has to be able to prove it.

**So the tables below are generated.** `tools/gen_merged_lands_table.py` parses
the Rust source, looks each function up in a hand-written coverage map, and
**fails** if any function has no entry or any entry names no function. Run it
with `--check` against a copy of the source to re-verify:

```
python tools/gen_merged_lands_table.py --src ../merged_lands-main/src --check
```

`tests/test_merged_lands_coverage.py` runs the same check whenever the source
tree is present.

Merged Lands is MIT (David Von Derau, 2022). Licence text vendored at
`License/MergedLands/LICENSE`; see `CREDITS.md`.

Status column:

- **ported** — the behaviour exists here.
- **verified** — ported, *and* checked statement by statement against the Rust
  rather than by matching names. Used where the details decide the output:
  exact formulas, rounding, precedence, ordering.
- **gap** — ported only after this audit found it missing. Nine of these; each
  is written up below.
- **absorbed** — Rust scaffolding Python does not need (traits, iterators,
  newtype wrappers, `RelativeTo`/`RoundTo` arithmetic). Named individually so
  "absorbed" is a claim about a specific function, not a category.
- **replaced** — done differently and deliberately, with the reason given.
- **n/a** — belongs to a concern outside the merge (CLI parsing, logging).

Totals across the five tables: **113 ported, 46 absorbed, 11 gap, 10 verified,
7 n/a, 4 replaced = 191.** Eleven rows are marked *gap* against ten faults
because fault 5 and fault 2 both run through
`create_merged_lands_from_reference`.

## `land/` — 64 functions

| fn | file | status | here |
|---|---|---|---|
| `convert_terrain_map` | `land/conversions.rs` | absorbed | list comprehension |
| `vertex_normals` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_vertex_normals` |
| `vertex_colors` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_vertex_colors` |
| `world_map_data` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_world_map` |
| `texture_indices` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_texture_indices`, which de-swizzles the sixteen 4x4 blocks |
| `landscape_flags` | `land/conversions.rs` | ported | `diff.parse_landscape_flags` |
| `coordinates` | `land/conversions.rs` | ported | `diff.LandscapeLayers.from_record` |
| `Index2D::new` | `land/grid_access.rs` | absorbed | an `(x, y)` tuple |
| `Iterator for GridIterator2D<X, Y>::next` | `land/grid_access.rs` | absorbed | nested `range()` |
| `trait SquareGridIterator::iter_grid` | `land/grid_access.rs` | absorbed | `range()` |
| `trait GridAccessor2D::get` | `land/grid_access.rs` | ported | `RelativeGrid.value_at` |
| `trait GridAccessor2D::get_mut` | `land/grid_access.rs` | ported | `RelativeGrid.set_value` |
| `truncate_gradient` | `land/height_map.rs` | ported | `heights._fit`, clamping a delta to one signed byte |
| `calculate_vertex_heights` | `land/height_map.rs` | ported | `heights.encode_vertex_heights`. Verified: 400/400 real cells round-trip exactly, 0 clamps |
| `calculate_vertex_heights_tes3` | `land/height_map.rs` | ported | same function -- the two Rust variants differ only in return type |
| `calculate_height_map` | `land/height_map.rs` | ported | `heights.decode_heights_from_deltas`, the doubly-cumulative sum |
| `calculate_vertex_normals_map` | `land/height_map.rs` | ported | `heights.vertex_normals_from_heights` |
| `fix_coords` | `land/height_map.rs` | ported | edge reuse inside `vertex_normals_from_heights` |
| `try_calculate_height_map` | `land/height_map.rs` | replaced | the sanity assert became `heights.round_trips`, which returns rather than aborting the run on one cell |
| `LandscapeDiff::is_modified` | `land/landscape_diff.rs` | ported | `LandscapeDiff.is_modified` |
| `LandscapeDiff::modified_data` | `land/landscape_diff.rs` | ported | `LandscapeDiff.modified_data` |
| `LandscapeDiff::from_reference` | `land/landscape_diff.rs` | gap | `pipeline.inherit_reference_layers` -- fault 2 |
| `LandscapeDiff::from_difference` | `land/landscape_diff.rs` | ported | `diff.diff_against_reference` |
| `LandscapeDiff::apply_mask` | `land/landscape_diff.rs` | ported | `seams.mask_normals_to_moved_heights` |
| `LandscapeDiff::calculate_differences_with_mask` | `land/landscape_diff.rs` | ported | `diff_against_reference` |
| `LandscapeDiff::calculate_differences` | `land/landscape_diff.rs` | ported | `diff_against_reference` |
| `LandscapeDiff::calculate_reference` | `land/landscape_diff.rs` | ported | `pipeline.inherit_reference_layers` |
| `Vec2<T>::new` | `land/terrain_map.rs` | absorbed | a coordinate tuple |
| `From<[T; 2]> for Vec2<T>::from` | `land/terrain_map.rs` | absorbed | a coordinate tuple |
| `From<Vec2<T>> for [T; 2]::from` | `land/terrain_map.rs` | absorbed | a coordinate tuple |
| `Vec3<T>::new` | `land/terrain_map.rs` | absorbed | components are interleaved in one flat list |
| `From<[T; 3]> for Vec3<T>::from` | `land/terrain_map.rs` | absorbed | interleaved flat list |
| `From<Vec3<T>> for [T; 3]::from` | `land/terrain_map.rs` | absorbed | interleaved flat list |
| `GridAccessor2D<U> for TerrainMap<U, T>::get` | `land/terrain_map.rs` | ported | `RelativeGrid.value_at` |
| `GridAccessor2D<U> for TerrainMap<U, T>::get_mut` | `land/terrain_map.rs` | ported | `RelativeGrid.set_value` |
| `SquareGridIterator<T> for TerrainMap<U, T>::iter_grid` | `land/terrain_map.rs` | absorbed | `range()` |
| `From<LandscapeFlags> for LandData::from` | `land/terrain_map.rs` | ported | `diff.parse_landscape_flags`, including the derived world-map rule |
| `IndexVTEX::new` | `land/textures.rs` | absorbed | a plain `int` |
| `IndexVTEX::as_u16` | `land/textures.rs` | absorbed | a plain `int` |
| `From<IndexVTEX> for f64::from` | `land/textures.rs` | absorbed | a plain `int` |
| `RelativeTo for IndexVTEX::subtract` | `land/textures.rs` | absorbed | integer subtraction |
| `RelativeTo for IndexVTEX::add` | `land/textures.rs` | absorbed | integer addition |
| `IndexLTEX::new` | `land/textures.rs` | absorbed | a plain `int` |
| `IndexLTEX::as_u16` | `land/textures.rs` | absorbed | a plain `int` |
| `From<IndexLTEX> for IndexVTEX::from` | `land/textures.rs` | ported | `textures.vtex_of` |
| `TryFrom<IndexVTEX> for IndexLTEX::try_from` | `land/textures.rs` | ported | `textures.ltex_of`, which returns `None` for the reserved 0 |
| `RemappedTextures::with_capacity` | `land/textures.rs` | absorbed | a `dict` |
| `RemappedTextures::new` | `land/textures.rs` | ported | `KnownTextures.translation` |
| `RemappedTextures::from` | `land/textures.rs` | ported | `textures.compact_textures`, built from the values actually painted |
| `RemappedTextures::try_remapped_index` | `land/textures.rs` | ported | `dict.get` |
| `RemappedTextures::remapped_index` | `land/textures.rs` | ported | `textures.translate_indices`, which passes an unknown value through and reports it rather than repainting the terrain |
| `KnownTexture::id` | `land/textures.rs` | ported | `KnownTexture.identifier` |
| `KnownTexture::index` | `land/textures.rs` | ported | `KnownTexture.index` |
| `KnownTexture::clone_landscape_texture` | `land/textures.rs` | ported | `emit.build_texture_records` |
| `KnownTexture::texture_index` | `land/textures.rs` | ported | field read in `observe` |
| `KnownTextures::new` | `land/textures.rs` | ported | `KnownTextures.__init__` |
| `KnownTextures::sorted` | `land/textures.rs` | ported | `KnownTextures.sorted` |
| `KnownTextures::update_texture` | `land/textures.rs` | verified | `KnownTextures.observe`: a later plugin's *different* file name wins and takes ownership; an absent one changes nothing |
| `KnownTextures::add_texture` | `land/textures.rs` | verified | `observe`: keyed on the `LTEX` id, so the first plugin to declare an id fixes its shared index |
| `KnownTextures::add_remapped_texture` | `land/textures.rs` | ported | `observe` returns the plugin's translation |
| `KnownTextures::remove_unused` | `land/textures.rs` | ported | `textures.compact_textures` |
| `KnownTextures::len` | `land/textures.rs` | ported | `KnownTextures.__len__` |
| `KnownTextures::next_texture_index` | `land/textures.rs` | ported | `len(self._by_id)` |
| `KnownTextures::add_next_texture` | `land/textures.rs` | gap | `observe`. Its `DELETED` assert became `diff.is_deleted`, which skips and logs -- fault 9 |

## `merge/` — 53 functions

| fn | file | status | here |
|---|---|---|---|
| `merge_cell_into` | `merge/cells.rs` | ported | `cells.merge_cell_into`, plus a bug fix: the original clones `new` from itself, so region, map colour, water height and atmosphere never update |
| `merge_cells_into` | `merge/cells.rs` | ported | `cells.merge_cells` |
| `merge_cells` | `merge/cells.rs` | ported | `cells.merge_cells` |
| `trait ConflictResolver::average` | `merge/conflict.rs` | ported | `merge.average_delta` |
| `Default for ConflictParams::default` | `merge/conflict.rs` | ported | `merge.ConflictParams` defaults: 0.3 / 10.0 / 64.0 |
| `Default for ConflictParams::classify_conflict` | `merge/conflict.rs` | verified | `merge.average_delta`. Weight `\|lhs\|/(\|lhs\|+\|rhs\|)` raised to 1.5 and renormalised; severity measured from the *signed* minimum against `min(max(0.3*min, 10), 64)`; `RoundTo` is `as i32`, which truncates toward zero, and so does `int()` |
| `Default for ConflictParams::average` | `merge/conflict.rs` | ported | `average_delta`; equal values return no conflict |
| `ConflictResolver for Vec3<T>::average` | `merge/conflict.rs` | ported | `merge.merge_layer` resolves each component; the vertex is Major if any component is |
| `MergeStrategy for IgnoreStrategy::apply` | `merge/ignore_strategy.rs` | ported | `ConflictStrategy.IGNORE`: contested vertices keep the earlier edit |
| `trait MergeStrategy::apply` | `merge/merge_strategy.rs` | ported | `merge.merge_layer` |
| `trait MergeStrategy::apply_strategy` | `merge/merge_strategy.rs` | ported | `pipeline._fold`: with one side absent, the other is taken whole |
| `trait MergeStrategy::apply_preferred_strategy` | `merge/merge_strategy.rs` | ported | `merge._resolve_for` |
| `trait MergeStrategy::apply_merge_strategy` | `merge/merge_strategy.rs` | verified | `merge.DEFAULT_STRATEGY`: resolve for heights, normals, colours and world map; overwrite for textures |
| `MergeStrategy for OverwriteStrategy::apply` | `merge/overwrite_strategy.rs` | ported | `ConflictStrategy.OVERWRITE`: contested vertices take the later edit |
| `SquareGridIterator<T> for RelativeTerrainMap<U, T>::iter_grid` | `merge/relative_terrain_map.rs` | absorbed | `range()` |
| `RelativeTerrainMap<U, T>::default` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.__init__` with a zero reference |
| `RelativeTerrainMap<U, T>::empty` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.__init__` |
| `RelativeTerrainMap<U, T>::from_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.from_difference` |
| `RelativeTerrainMap<U, T>::differences` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.changed_vertices` |
| `RelativeTerrainMap<U, T>::get_value` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.value_at` |
| `RelativeTerrainMap<U, T>::set_value` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.set_value` |
| `RelativeTerrainMap<U, T>::get_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.delta_at` / `deltas_at` |
| `RelativeTerrainMap<U, T>::set_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.set_deltas` |
| `RelativeTerrainMap<U, T>::has_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.has_difference`, one flag per *vertex* not per component |
| `RelativeTerrainMap<U, T>::clean_all` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.clear` over the grid |
| `RelativeTerrainMap<U, T>::clean_some` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.clear` |
| `RelativeTerrainMap<U, T>::to_terrain` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.to_flat` / `to_rows` |
| `trait IsModified::is_modified` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.is_modified` |
| `trait IsModified::num_differences` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.num_differences` |
| `IsModified for RelativeTerrainMap<U, T>::is_modified` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.is_modified` |
| `IsModified for RelativeTerrainMap<U, T>::num_differences` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.num_differences` |
| `IsModified for OptionalTerrainMap<U, T>::is_modified` | `merge/relative_terrain_map.rs` | ported | `LandscapeDiff.is_modified`, which treats an absent layer as unmodified |
| `IsModified for OptionalTerrainMap<U, T>::num_differences` | `merge/relative_terrain_map.rs` | ported | `LandscapeDiff.num_differences` |
| `IsModified for OptionalTerrainMap<U, T>::recompute_vertex_normals` | `merge/relative_terrain_map.rs` | gap | `pipeline.resolve_normals`: recomputes where a height moved, inherits the authored normal where it did not -- fault 4 |
| `trait RelativeTo::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `trait RelativeTo::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for i32::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for i32::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for u8::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for u8::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for i8::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for i8::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for u16::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for u16::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for Vec3<T>::subtract` | `merge/relative_to.rs` | absorbed | per-component subtraction over the interleaved list |
| `RelativeTo for Vec3<T>::add` | `merge/relative_to.rs` | absorbed | per-component addition over the interleaved list |
| `MergeStrategy for ResolveConflictStrategy::apply` | `merge/resolve_conflict_strategy.rs` | verified | `ConflictStrategy.RESOLVE`. Averages the *deltas*, not the values; only-one-side-moved takes that side whole |
| `trait RoundTo::round_to` | `merge/round_to.rs` | absorbed | `int()`, which truncates toward zero exactly as Rust's `as` does |
| `RoundTo<i32> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<i8> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<u8> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<u16> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<IndexVTEX> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |

## `repair/` — 14 functions

| fn | file | status | here |
|---|---|---|---|
| `has_difference` | `repair/cleaning.rs` | verified | `cleaning.differs`. The Rust routes through `ConflictResolver::average`, which returns `None` exactly when the two values are equal -- so it is an equality test, and it is false the moment either side is absent |
| `has_any_difference` | `repair/cleaning.rs` | gap | `cleaning.differs_any`. Tests all five layers; we tested heights -- fault 6 |
| `clean_landmass_diff` | `repair/cleaning.rs` | gap | `cleaning.clean_landmass`. Its opening `assert_eq!(repair_landmass_seams(landmass), 0)` -- the merge's post-condition -- was not ported at all; it is now `seams.find_tears` via `pipeline._check_borders` -- fault 10 |
| `clean_known_textures` | `repair/cleaning.rs` | ported | `textures.compact_textures`, plus a bug fix: the original's second loop is named `plugins` but iterates `masters`, so a mod's `LTEX` file name never reaches the output |
| `update_known_textures` | `repair/cleaning.rs` | ported | `KnownTextures.observe` |
| `add_vertex_colors` | `repair/debugging.rs` | ported | `debug_colors.paint_conflicts` |
| `add_debug_vertex_colors_to_landscape` | `repair/debugging.rs` | ported | `debug_colors.paint_conflicts` |
| `add_debug_vertex_colors_to_landmass` | `repair/debugging.rs` | ported | `debug_colors.paint_conflicts` over the landmass |
| `coords_with_offset` | `repair/seam_detection.rs` | absorbed | tuple arithmetic |
| `push_back_neighbors` | `repair/seam_detection.rs` | ported | `seams._border_pairs` |
| `sort_pair` | `repair/seam_detection.rs` | ported | `seams._border_pairs` orders each pair so a border is visited once |
| `repair_corner_seams` | `repair/seam_detection.rs` | gap | `seams.repair_corners`, extended with anchoring for corners shared with terrain outside the merge -- fault 5 |
| `repair_landmass_seams` | `repair/seam_detection.rs` | ported | `seams.repair_seams`. Called a second time by `clean_landmass_diff` as an assertion; that use is `seams.find_tears` |
| `try_repair_seam` | `repair/seam_detection.rs` | verified | `seams.repair_edges`, averaging through `seams.mean`, which truncates toward zero to match Rust's integer `/` rather than Python's flooring `//` |

## `io/` — 33 functions

| fn | file | status | here |
|---|---|---|---|
| `Default for MergeSettings::default` | `io/meta_schema.rs` | ported | `meta.MergeSettings` field defaults: `included=True`, `Auto` |
| `Default for MergeSettings::default_bool_true` | `io/meta_schema.rs` | absorbed | `included: bool = True` on the dataclass |
| `Default for MergeSettings::skip_default` | `io/meta_schema.rs` | replaced | serde's write-skip. We never write a settings file, only the `MergedLands` marker -- see `meta.write_merged_marker` |
| `parse_records` | `io/parsed_plugins.rs` | replaced | `service._records_via`: tes3conv JSON, not a TES3 reader |
| `read_lines` | `io/parsed_plugins.rs` | ported | `tools.survey_landscape.read_order`, which refuses a file with no plugin entries rather than returning every line |
| `is_esm` | `io/parsed_plugins.rs` | verified | `service._split_order`. Case-insensitive extension test, plus `.omwgame` |
| `is_esp` | `io/parsed_plugins.rs` | verified | `cleaning.is_mod`. Case-insensitive, plus `.omwaddon` |
| `sort_plugins` | `io/parsed_plugins.rs` | replaced | load order comes from the toolkit's own sort, not file mtimes |
| `meta_name` | `io/parsed_plugins.rs` | ported | `meta.meta_path_for` |
| `ParsedPlugin::empty` | `io/parsed_plugins.rs` | absorbed | `PluginRecords(name, [])` |
| `ParsedPlugin::from` | `io/parsed_plugins.rs` | absorbed | dataclass constructor |
| `PartialEq<Self> for ParsedPlugin::eq` | `io/parsed_plugins.rs` | absorbed | dataclass equality |
| `Hash for ParsedPlugin::hash` | `io/parsed_plugins.rs` | absorbed | plugins are keyed by their name string |
| `Hash for ParsedPlugin::read_ini_file` | `io/parsed_plugins.rs` | ported | `read_order` also parses `Morrowind.ini` `GameFile` lines |
| `ParsedPlugins::check_dir_exists` | `io/parsed_plugins.rs` | ported | `service.build_merged_lands` refuses an empty folder list; `service.resolve_plugin` skips a folder it cannot read |
| `ParsedPlugins::new` | `io/parsed_plugins.rs` | ported | `service.build_merged_lands` reading phase, over every `data=` folder |
| `save_resized_image` | `io/save_to_image.rs` | ported | `conflict_image._blit`, scaling by `SCALE = 4` |
| `GridAccessor2D<P> for ImageBuffer<P, Container>::get` | `io/save_to_image.rs` | absorbed | list indexing |
| `GridAccessor2D<P> for ImageBuffer<P, Container>::get_mut` | `io/save_to_image.rs` | absorbed | list assignment |
| `trait SaveToImage::save_to_image` | `io/save_to_image.rs` | ported | `conflict_image.cell_conflict_image` |
| `SaveToImage for RelativeTerrainMap<Vec3<i8>, T>::save_to_image` | `io/save_to_image.rs` | ported | same, vertex normals layer |
| `SaveToImage for RelativeTerrainMap<u16, T>::save_to_image` | `io/save_to_image.rs` | ported | same, texture indices layer |
| `SaveToImage for RelativeTerrainMap<Vec3<u8>, T>::save_to_image` | `io/save_to_image.rs` | ported | same, vertex colours layer |
| `SaveToImage for RelativeTerrainMap<Vec3<u8>, T>::calculate_min_max` | `io/save_to_image.rs` | ported | range scan inside `cell_conflict_image` |
| `SaveToImage for RelativeTerrainMap<u8, T>::save_to_image` | `io/save_to_image.rs` | ported | same, world map layer |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_to_image` | `io/save_to_image.rs` | ported | same, height map layer |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_image` | `io/save_to_image.rs` | ported | `wraithguard.images.png` writer -- no image crate needed |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_landscape_images` | `io/save_to_image.rs` | ported | `conflict_image.cell_conflict_image` |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_landmass_images` | `io/save_to_image.rs` | ported | `conflict_image.landmass_conflict_image` |
| `convert_landscape_diff_to_landscape` | `io/save_to_plugin.rs` | ported | `emit.build_landscape_record`. Diverges on flags: we declare only the layers we actually write |
| `convert_landmass_diff_to_landmass` | `io/save_to_plugin.rs` | ported | `service._build_records` |
| `to_master_record` | `io/save_to_plugin.rs` | gap | `service._contributors` + `service._write`. The master list is the plugins that *contributed*, not the load order's masters -- fault 8 |
| `save_plugin` | `io/save_to_plugin.rs` | gap | `service._write`, which now also writes the `.mergedlands.toml` marker beside the output -- fault 7 |

## `main.rs` — 27 functions

| fn | file | status | here |
|---|---|---|---|
| `Landmass::new` | `main.rs` | ported | `landmass.Landmass` |
| `Landmass::insert_land` | `main.rs` | ported | `Landmass.cells[coords] = ...` |
| `Landmass::sorted` | `main.rs` | ported | `sorted(landmass.cells)` |
| `LandmassDiff::new` | `main.rs` | ported | `pipeline.MergeOutcome` |
| `LandmassDiff::sorted` | `main.rs` | ported | `sorted(outcome.cells)` |
| `From<CliLevelFilter> for LevelFilter::from` | `main.rs` | n/a | log level mapping; the toolkit configures `logging` itself |
| `Cli::read_args` | `main.rs` | n/a | `tools/build_merged_lands.py` argparse |
| `Cli::plugins` | `main.rs` | ported | `tools.survey_landscape.read_order` |
| `Cli::should_write_log_file` | `main.rs` | n/a | the toolkit's own logging |
| `Cli::merged_lands_dir` | `main.rs` | ported | `--conflicts-dir` |
| `Cli::data_files_dir` | `main.rs` | ported | `service.resolve_plugin` over every `data=` folder |
| `Cli::output_file_dir` | `main.rs` | ported | `--out` |
| `Cli::stack_size` | `main.rs` | n/a | Rust thread stack sizing; the merge here is iterative, not recursive |
| `Cli::main` | `main.rs` | n/a | entry point |
| `Cli::wait_for_user_exit` | `main.rs` | n/a | console behaviour |
| `Cli::merge_all` | `main.rs` | verified | `pipeline.finish` + `service.build_merged_lands`, in the same order: reference, diff, merge, repair, clean, textures, convert, save |
| `Cli::init_log` | `main.rs` | n/a | logging setup |
| `Cli::try_copy_landscape_and_remap_textures` | `main.rs` | ported | `landmass._decode_cell` + `textures.translate_indices` |
| `Cli::try_create_landmass` | `main.rs` | ported | `landmass.build_reference` / `landmass.plugin_differences` |
| `Cli::merge_tes3_landscape` | `main.rs` | gap | `landmass.merge_master_layers`: masters combine per *layer*, flags unioned -- fault 3 |
| `Cli::merge_tes3_landmasses` | `main.rs` | ported | `landmass.build_reference` |
| `Cli::find_allowed_data` | `main.rs` | gap | `diff.diff_against_reference` intersects `allowed` with the record's declared flags -- fault 1 |
| `Cli::find_landmass_diff` | `main.rs` | ported | `landmass.plugin_differences` |
| `Cli::merge_landscape_diff` | `main.rs` | ported | `pipeline._fold_change` |
| `Cli::merge_landmass_into` | `main.rs` | ported | `pipeline.merge_landmass` |
| `Cli::create_tes3_landmass` | `main.rs` | ported | `landmass.build_reference` |
| `Cli::create_merged_lands_from_reference` | `main.rs` | gap | `pipeline.inherit_reference_layers` + `pipeline.add_reference_neighbours` -- faults 2 and 5 |

## The fourteen faults this audit found

Twelve would have shipped. Faults 12 and 13 were in the check that caught the
eleventh -- both false alarms that blocked a valid write rather than letting a
bad one through, which is the right way round for a check to fail. Fault 14 was
found by the game. None raised, crashed, or failed an existing test.
Every one was inside a function that a file-level audit had already marked
"ported".

Faults 1 to 5 came from the first pass, which read every function once. Faults 6
to 9 came from the second, which compared the remaining ones statement by
statement — and which only happened because the first table's own counts did not
add up. Fault 10 came from the third, which went through the functions the
second pass had *mapped* but never run -- and fault 11 came from running the
whole thing against a real load order, where fault 10's new post-condition
caught it.

### 1. Undeclared layers were merged as data

`find_allowed_data` starts from the record's own `landscape_flags`. We started
from the caller's choice alone. `DATA` says which grids a `LAND` record uses and
the engine ignores the rest — but tes3conv emits *every* grid, so an undeclared
one arrives full of zeros and diffed as *this mod flattened the terrain and
painted it black*.

Not hypothetical: of 290 real landscape records, **21 carry texture data the
flags do not declare, 20 carry vertex colours, 6 carry heights.**

Fixed in `diff_against_reference`; pinned by `TestUndeclaredLayersAreIgnored`.

### 2. Unchanged layers were dropped from the output

`create_merged_lands_from_reference` seeds every cell with the reference via
`from_reference`, so `to_terrain()` returns real terrain even for layers nobody
edited. We carried only *changed* layers.

A merged `LAND` record replaces the whole record. Carrying heights alone leaves
the cell with no texture data at all — the mod's and the game's both gone, since
the record holding them has been superseded. **Untextured terrain, silently.**

Measured before the fix: of 24 cells written from two Solstheim mods, **13 lost
textures the reference had, 14 lost vertex colours, 12 lost the world map.**
After: zero. The plugin grew from 188 KB to 729 KB, which is the correct size
for terrain that is actually complete.

Fixed by `pipeline.inherit_reference_layers`; pinned by
`TestUnchangedLayersSurvive`.

### 3. Masters were combined wholesale, not per layer

`merge_tes3_landscape` starts from the earlier master and replaces only the
layers the later one both *declares* and *carries*, unioning the flags. We
replaced the whole record.

A master redefining a cell with heights alone therefore erased the earlier
master's vertex colours and textures. Invisible on a stock install — Tribunal
has no landscape and Bloodmoon is Solstheim, so the vanilla three never overlap
— and live the moment `Tamriel_Data.esm`, `OAAB_Data.esm` or any other
master-flagged expansion is present, which is most large load orders.

Fixed in `landmass.merge_master_layers`; pinned by `TestMastersCombinePerLayer`.

### 4. Hand-authored normals were discarded

`recompute_vertex_normals` recomputes from the merged heights but keeps the
*original* normal at any vertex whose height did not change. We recomputed all
of them, discarding normals a mod authored deliberately to fake a lighting
effect its geometry does not produce.

Fixed in `pipeline.resolve_normals`, which also moves normal computation to
after seam repair and the slope limiter — they are a function of the final
heights, and computing them earlier would describe terrain that no longer
exists. Pinned by `TestNormalsFollowTheHeights`.

### 5. Torn borders against terrain outside the merge

Three faults in one place, each uncovered by fixing the last.

**Only modified cells were merged**, so a merged cell's border with untouched
vanilla was never reconciled — 16 borders disagreeing, worst 5,024 world units.
Fixed by borrowing reference neighbours before repair.

**Only orthogonal neighbours were borrowed.** A corner is shared by *four*
cells, so the diagonal was still absent and its corner unreconcilable. Fixed by
borrowing diagonally as well.

**Where a sharing cell genuinely is not written**, averaging the rest moves our
side of the ground and not theirs. Pinning all four was tried and is worse: the
cells we *do* write then keep three different values and tear against each other
(4 borders, worst 3,944). The rule that works is **anchoring** — the absent
ground is not going to move, so it decides, and every present cell adopts the
reference height. Where the absent cell has no terrain at all, there is nothing
to tear against and the present cells simply agree among themselves.

Result on the Solstheim pair: **0 torn borders**, against 16 at the start.
Pinned by `TestNoTornBorders`.

### 6. Cleaning judged on heights alone

`has_any_difference` tests heights, normals, world map, colours **and**
textures. We tested heights.

A cell where one mod repainted the textures and another recoloured the vertices
has heights identical to the reference, so it was dropped as "unmodified". Once
the merged record is gone the load order resolves that cell by last-wins, and
one of the two edits disappears. Nothing in the merge reports it — heights are
what the merge talks about, and heights were fine.

Fixed in `cleaning.differs_any`. Grids are compared by 32-byte BLAKE2b digest
rather than by value: holding five real grids for every single-editor cell would
cost about 120 KB a cell against 17 KB for heights alone — over a gigabyte on a
large load order — for a comparison that only ever asks *are these the same*.
Pinned by `TestCleaningJudgesEveryLayer`.

### 7. The output was not marked as generated

`save_plugin` writes a `.mergedlands.toml` beside the plugin with
`meta_type = MergedLands`, and `merge_all` skips any plugin whose meta says
that. We read the marker and never wrote one.

`Merged Lands.esp` is an `.esp` that edits every cell it wrote and loads last,
so the second run read it as a mod and reconciled its terrain as one more
opinion — a merge of a merge. Nothing fails; the terrain drifts a little further
from what any author wrote on every run, and the drift is invisible because the
tool has no memory of the previous output.

Fixed by `meta.write_merged_marker`, called from `service.build_merged_lands`
immediately after the write. Failure to write it raises rather than warns: a
merged plugin with no marker beside it is a trap for the next run.

### 8. Every master was declared, contributors were not

`save_plugin` builds its master list from the plugins that actually
contributed — every plugin behind a surviving `LTEX`, every plugin that edited a
written cell — and sorts it by load order. We declared the load order's masters.

Both halves were wrong. Declaring twenty-seven masters when five were read is
noise, and *not* declaring the `.esp` mods the terrain came from means the
merged file will happily load without them, describing edits to land that is no
longer there. A TES3 master list is a dependency list; nothing stops an `.esp`
being on it, and putting it there is what makes the engine refuse rather than
render.

We add one thing the original gets for free through its `LandscapeDiff.plugins`
chain: the master supplying each written cell's *reference* terrain is declared
too. A `LAND` record is an override of the record in the file that first defined
that cell, and a merged file that does not name `Morrowind.esm` is not a patch
of Morrowind.

Fixed in `service._contributors`; pinned by `TestDeclaredMasters`.

### 9. Deleted records were read as terrain

`add_next_texture` and `merge_tes3_landscape` both assert on a record carrying
`ObjectFlags::DELETED`. We had no check at all, so a deleted `LTEX` entered the
shared table — reinstating a texture a mod removed and occupying an index — and
a deleted `LAND` was diffed as an edit, merging whatever stale grid happened to
be in the file.

Fixed by `diff.is_deleted`, consulted in `textures.observe` and
`landmass._landscape_records`. We skip and log where the original aborts:
ending a nine-hundred-mod merge because one plugin deleted one texture is not a
useful outcome, and skipping leaves the reference terrain standing, which is
what a terrain-only patch should do with a record it cannot interpret.

### 10. The merge had no post-condition

`clean_landmass_diff` opens with
`assert_eq!(repair_landmass_seams(landmass), 0)`. It repairs the seams, then
repairs them **again** and requires the second pass to find nothing. We ported
the repair and dropped the assertion — the function was marked "ported" because
the cleaning it does is there, and the first line of it is not cleaning.

That left the one defect a player sees in the first minute with no check at
all: a wall or a chasm along a cell boundary, in a game where cells are 8,192
units across and every boundary is somewhere you walk.

And it matters more here than in the original, because more runs after the
repair. The slope limiter moves vertices. Feathering moves vertices. Cleaning
removes whole cells. Each is written to preserve borders — `_shift` moves every
copy of a shared vertex in lockstep and refuses outright when a sharing cell is
absent; cleaning keeps any single-source cell the repair moved — and each is a
place a future change could quietly stop doing so. The 0-tear result the port
was signed off with came from a throwaway script, not from anything that would
run again.

`seams.find_tears` is now the post-condition, called from
`pipeline._check_borders` after everything has finished moving and against the
cells that will actually be **written**, not the working set. Borders with cells
the merge is *not* writing are checked too, against the reference: a written
cell does not get to be merely self-consistent, because the ground next door
still exists and is not going to move.

`service.build_merged_lands` **refuses to write** when a tear survives, naming
the border. Shipping visibly broken terrain is worse than failing, and this is a
defect here rather than anything the user did.

Pinned by `TestNothingTearsAtTheEnd`, which includes a negative control —
the same merge with repair disabled, which must report exactly one tear.
A post-condition that cannot fail is decoration.

### 11. Borrowed vanilla cells were averaged, not adopted

`add_reference_neighbours` is ours, not the original's -- Merged Lands seeds
the whole reference and cleans at the end, we borrow only the ring that borders
an edit. But a borrowed cell was then treated as an equal party: `repair_edges`
averaged the merged cell against it and moved both.

That moves *vanilla*. And the cell one further out -- not borrowed, still
holding its original heights -- does not follow. The tear is not removed; it is
relocated one cell away from the edit, into terrain no mod asked to change.

Measured on a real 27-master, 940-mod order: **25 borders genuinely
disagreeing.** (The run reported 783; 758 of those were fault 12, below. The
initial write-up of this fault quoted the whole 783 and the 17,560-unit figure,
both of which belonged to the false positive. The real defect is smaller than
that and worth stating at its true size.)

It is small because `try_repair_seam` only moves the 65 vertices *on* a shared
border, so averaging a merged cell against a borrowed one does not disturb the
borrowed cell's other three sides. What does tear is corners: a corner vertex is
shared by four cells, and `repair_corner_seams` moves it in all of them.

A borrowed cell is now **authoritative**: it is a copy of what the game already
has, so the merged side adopts its heights whole and it never changes. Two
consequences beyond the fix -- the slope limiter has to respect the same rule
(it runs afterwards, and moving one of those vertices would undo the repair),
and a borrowed cell now always survives to cleaning unmodified, so it is
dropped rather than written.

**The guard has to hold in two places, and the first attempt only held in one.**
Seam repair honouring it is not enough: the slope limiter sweeps the same grids
afterwards, and a large mod-versus-vanilla difference gives it plenty to correct.
`_is_movable` learned the rule, but `limit_slopes` called it without passing
`authoritative`, so the check was inert and the limiter went on editing vanilla.
An 800-unit test difference was too small to expose that; 18,000 -- the scale a
real load order reaches -- was not.

Pinned by `TestBorrowedCellsAreNotMoved`: a negative control that removes the
guard and checks vanilla gets dragged, a test that runs repair and the limiter
in sequence and requires vanilla untouched after *both*, and an end-to-end case
at 18,000 units.

### 12. The post-condition invented tears where cleaning had dropped a cell

Fault 10 added the check the port was missing. It was then run *after*
cleaning, on the cells that survive -- which sounds stricter and is simply
wrong.

Cleaning drops a cell exactly when the load order already delivers that
terrain: either nothing edited it, or one mod did and its own record produces
the same result. The ground is still there in the game; it just comes from a
different file. Treating it as absent and falling back to the reference
measures a merged cell against *vanilla* when the terrain next door is the
mod's, and reports a tear the size of that mod's edit.

On the same 940-mod order: **758 borders reported, the worst 17,560 units, for
borders that were intact** -- and the check refused to write the plugin over it.
The tell was that the worst tear did not move at all when fault 11 was fixed,
because it was never a tear.

The check now runs on the repaired landmass, before cleaning. That is also the
stronger statement, not the weaker one: cleaning cannot open a border the check
closed, because it only ever removes a record whose content the load order
reproduces exactly.

Pinned by `TestTheCheckDoesNotInventTears`, which includes the case that was
misreported *and* an unrepaired landmass that must still be caught -- a check
relaxed into uselessness would pass the first test alone.

### 13. The post-condition blamed the tool for vanilla's own seams

Fixing fault 12 moved the check before cleaning, which put the borrowed cells
in front of it for the first time. Those are the cells the merge does not edit
and will not write, and `repair_edges` refuses to move either side of a border
two of them share -- whatever they disagree about is the game's own, predates
the merge, and survives whether or not the tool runs. Two adjacent masters
disagreeing at a province boundary is an ordinary thing in a 27-master order.

The check did not know that, and reported them: **62 borders, the worst 2,648
units**, on terrain nothing in the output touches. It then refused to write.

`find_tears` now takes the borrowed set and skips borders where *both* sides are
in it. A border with *one* of them is still checked -- that is the entire reason
they were borrowed, and it is what fault 11 was about.

Pinned by `TestVanillaSeamsAreNotOurs`, which checks all four combinations:
neither side ours, one side ours, both sides ours, and a borrowed cell's border
with terrain further out.

### 14. A Skyrim plugin was read as Morrowind terrain

The first merged plugin that ever passed every check crashed OpenMW on load::

    ESM4::Reader::updateModIndices required dependency 'Skyrim.esm' not found

`ESM4` is OpenMW's Oblivion/Skyrim reader. It only runs on a file whose header
magic is `TES4`, so the merged plugin had dragged a Skyrim plugin into the load.

`native.py` had **no format check at all**, and every step of the chain worked
exactly as designed:

1. Skyrim has `LAND` records too, so the byte pre-scan matched.
2. tes3conv correctly refused the file -- `TES4` is not a format it reads.
3. That refusal is precisely when we fall back to the native reader.
4. Which walked it with a 16-byte record header. Skyrim's are 24. It did not
   fail; it *succeeded*, inventing landscape out of whatever the bytes spelled.
5. Those cells entered the merge, so the plugin was declared a contributing
   master -- and OpenMW then had to load it.

The tolerance that makes this reader worth having -- *skip anything you do not
recognise, by its declared length* -- is exactly what makes it dangerous on a
format whose lengths are laid out differently. It survives unknown **records**;
it must refuse unknown **formats**.

`read_landscape_records` now raises unless the file begins `TES3`, naming what
it found instead, and `has_landscape` returns false for the same reason rather
than sending it down the fallback path.

Pinned by `TestForeignFormatsAreRefused`, using a genuine `TES4` fixture with a
`LAND` record and a `Skyrim.esm` master -- the shape that caused this. Adding
the guard also broke five older tests that built plugins with no header at all,
which no real plugin does; those fixtures were wrong and were fixed rather than
the guard relaxed.

**This is the first fault the game found.** Twelve were found by reading, one by
running the tool, and this one needed the engine. It is the argument for the
last gate being a load, not a test.

## The bug that stopped a real load order

Separately from the parity sweep, the first run against a real 27-master, 940-mod
order failed at `could not read master distant_seafloor_2.00.esm`, and the
message was all the user got. Two faults combined:

* **One data folder was searched.** OpenMW composes a load order from many
  `data=` directories — one per mod is normal — and the toolkit passed whichever
  single folder held the first plugin it found. Every master installed elsewhere
  was unreachable. Fixed by `service.resolve_plugin`, which searches every scan
  directory and matches case-insensitively, because `openmw.cfg` and the
  filesystem routinely disagree (`RepopulatedMorrowind.ESM` against
  `…​.esm`).
* **The reason was discarded.** `_records_via` collapsed a converter error, a
  600-second timeout, a JSON parse failure and an out-of-memory into the same
  empty list, and `stderr` was never read. It now returns the reason, and the
  master that stops the merge says which of those happened.

A mod that cannot be found is no longer fatal either — a load order may list
something the user removed. A *master* still is: it is the reference terrain
everything else is measured against.

## The other direction: what is ours

The tables above prove *their* 191 functions all landed somewhere here. They say
nothing about the reverse, and the reverse is now the larger risk. Of **158
functions in `wraithguard/land`, only 83 trace to a Rust counterpart.**

Most of the remaining 75 is Python plumbing -- flattening helpers, digest
properties, report accessors, the tes3conv-JSON byte packing that replaces
`tes3`'s own `Save`. But three modules move vertices Merged Lands never moves,
and this is where "faithful port" stops being the right description:

| module | what it does that the original does not |
|---|---|
| `slope.py` | **No counterpart at all.** The original clamps at encode time and accepts the drift; this conditions the terrain first so it does not have to. It moved 241,342 vertices on a real load order |
| `seams.py` | The seam repair is a port; the **inward feathering, the corner anchoring, the authoritative rule and `find_tears` are not.** Each moves vertices, or refuses to, on rules the original has no notion of |
| `curvature.py` | No counterpart. Opt-in only -- `ConflictStrategy.CURVATURE` is never a default, so it changes nothing unless a `.mergedlands.toml` asks for it |

Everything else is marked `ours-aux`: no counterpart, but it decides no vertex
value. `native.py` reads records. `service.py` sequences the run.

`tools/gen_merged_lands_table.py` carries this classification as `MODULES`, and
`TestTheReverseDirection` fails if a module in `wraithguard/land` has no entry,
or if a *fourth* module ever joins the terrain-changing list. A short, named
list is the only thing that makes the claim about this port honest.

**What could not be checked.** A runtime differential -- build Merged Lands,
run both on the same load order, diff the plugins -- is the verification this
port really wants, and it is not available: the repository ships source only,
it pins `nightly-2022-07-26`, and there is no Rust toolchain here. Everything
above is a static correspondence. It is why the twelve faults were found by
reading rather than by running, and why the thirteenth was found by you running
it.

## Deliberate divergences

| what | why |
|---|---|
| `Resolve` refused on `texture_indices` | averaging index 3 and 7 gives index 5 — a third, unrelated texture |
| new land never averaged | no common ancestor; blending two authored landscapes makes a third nobody wrote |
| unencodable gradient reported, not fatal | the original asserts and aborts the whole run on one cell |
| seam corrections feathered inward | the original's boundary-only repair left 125 unencodable vertices; feathering leaves 6 |
| slope limiter | **the original has none.** Its only gradient handling anywhere in `src/` is `truncate_gradient`, called at encode time, which clamps a delta to `i8` and moves on -- so wherever a merged step exceeds 1,016 units it silently writes terrain different from what it computed, and because heights are cumulative sums the shift carries along the rest of that row. `try_calculate_height_map` asserts the round-trip on *input* terrain, which came from a file and is encodable by construction; nothing checks the merged output. Our limiter conditions the terrain instead, and what it cannot fix it reports |
| reference *neighbours* borrowed, not the whole landmass | same borders reconciled, a fraction of the memory |
| `merge_cell_into` reads the incoming value | the original clones `new` from itself, so region, map colour, water height and atmosphere never update |
| `.omwaddon` counts as a mod | OpenMW's name for the same format; the original checks `.esp` only |
| deleted `LTEX`/`LAND` skipped, not fatal | the original asserts; one deleted record should not end a nine-hundred-mod merge |
| `landscape_flags` declare only live layers | the original always writes `0x1|0x2|0x4|0x8`. The `0x8` bit is unnamed in `tes3` and emitting it is rejected outright; declaring a layer whose grid is absent asks the engine to load data that is not there |
| master sizes from `stat()` | the original uses the on-disk (`file_real_size`) size; the header field is a sanity check, and the apparent size is what every other tool writes |
| `clean_known_textures` refreshes file names from mods too | the original iterates `parsed_plugins.masters` twice — the second loop is named `plugins` but reads `masters`, so a mod's `LTEX` file name never reaches the output |
