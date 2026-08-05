# Merged Lands port — fidelity audit against `src`

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
or measured improvements — never accidental. The existing `MERGED_LANDS_PORT.md`
and `MERGED_LANDS_FUNCTIONS.md` claims hold up.

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

## Bottom line

This is one of the more carefully-done ports I've audited: the completeness claim
is machine-checked against the source, the numeric edge cases (truncation
direction, off-by-one, the VHGT reset, the self-clone typo) are each handled with
an explanatory comment, and every divergence is a decision with a reason attached
rather than an accident. The port faithfully reproduces `merged_lands-main/src`.
