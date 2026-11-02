# glam in Wraithguard: what is done and what is next

From the audit (`WraithguardToolkit_glam_audit.md`, 2026-10-28), agreed to do. The rules that
come with it:

- **Stay on glam 0.30** until the `tes3` fork's own `^0.30` moves (a second glam is a second,
  incompatible `Vec3`). 0.34 needs Rust 1.85; Gardenfell's workspace says 1.74.
- **Arrays stay the stored and wire types**; glam is for arithmetic inside a function.
- **Same operations, same order**: the first changes are checked *bit for bit* against the lines
  they replace, not with a tolerance (a tolerance would let a changed order through).
- **No blanket feature enabling.** `approx` (tests of math only) and `bytemuck` are already on
  through the TES3 fork. Nothing else without a use and a test.

## Done (Gardenfell patch 0019)

| What | Where | Check |
|---|---|---|
| Terrain normals with `Vec3` | `gardenfell-tes3/src/land.rs`: `unit_vnml`, `normal_at`, `face_normal` | `mod glam_normals_agree`: the old code kept as the reference, compared bit for bit over random heights and normals, blank corners, degenerate triangles; face normals point up |
| Flat face normal of a stand-in mesh | `gardenfell-nif/src/nif.rs`: `draw_from_geom` | `tests/wraithguard_glam_face.rs`, bit for bit, with a point and a sliver |
| Landscape curvature drift (2026-11-02) | `viewcore/src/mland.rs` now calls `landcore::merge` and `landcore::curvature` (one copy of the blend and the curvature); `landcore/src/curvature.rs` in `glam::DVec3`, f64, a normal divided by its length (not times its reciprocal) | `mland::tests::landcore_gives_the_old_answers_bit_for_bit` (the old copy kept), `curvature::tests::glam_gives_the_old_answers_bit_for_bit` (flat, ramp, ridges, random) |

## Next, in this order (each its own change, with its tests first)

1. **The NIF transform pipeline** (`gardenfell-nif/src/nif.rs`: `apply`, `rotate`, `node_trafo`,
   `mul`): the riskiest, the broadest. glam is column-major with column vectors; the NIF code is a
   row-major 3x3 plus translation in 9..11 and a separate scale, and the root node's transform is
   skipped on purpose. Write the identity, translation, nonuniform rotation, nested scale and
   round-trip tests against the *current* code and real fixtures (skinned too) first; then a small
   internal transform type behind the same `[f32; 12]` output; benchmark only then. Skinning,
   visible drawing and collision walk must move together.
2. **`gardenfell-bytes/src/geom.rs`, `segdist.rs`**: no caller found in this snapshot; check
   before spending anything on them.

## Left alone on purpose

`viewcore/src/lod.rs` `radius` (flat-array scan), `meshopt-port` (a line-for-line port: its
numbers are its tests), `triaccel.rs` (the traversal order is the algorithm), and
`nif_order.rs`'s bit-pattern fingerprint (exact on purpose). `serde`, `rand`, `mint`, `encase`,
`rkyv`/`speedy`/`zerocopy`, `scalar-math` and `fast-math`: no use here.
