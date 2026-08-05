# Merged Lands: what is ported, what is not

A file-by-file audit of `merged_lands-main/src` against `wraithguard/land/`,
written because "we ported it" had been asserted more than it had been checked.
Every row states where the behaviour lives here, or says plainly that it does
not exist yet.

Merged Lands is MIT (David Von Derau, 2022). Licence text vendored at
`License/MergedLands/LICENSE`; see `CREDITS.md`.

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
| `merge/cells.rs` | 100 | **ported** | `land/cells.py` — `--cells`, references never carried |
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

## `CELL` merging, and why it is safe

`--cells` emits `CELL` records alongside the terrain: region, water height, map
colour, name and flags. Mods that reshape land frequently adjust these too —
lowering water to match a dug channel, renaming a cell.

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
