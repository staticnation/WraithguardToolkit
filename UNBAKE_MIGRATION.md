# Un-baking migration: model-space geometry + per-shape instancing

## Current status

Stages 1 and 2 are complete. `model_shapes()` retains local vertices plus
`node_world`, `build_instanced()` folds the node transform into instance
matrix composition, and the cell preview no longer uses the old per-placement
`build_scene()` bake path. Animated nodes now use the direct
`placement · above · L(t) · below` representation; the old inverse-rest delta
has been removed from both the Python payload and the JavaScript render loop.

The free-threaded parse optimization is also live:
`build_instanced(..., workers=N)` warms unique model parses concurrently, and the
cell preview enables it only on a GIL-free interpreter. The old baked builder
has been removed rather than retained as a second hot path.

Stage 3/4 are now effectively complete for the application hot path: terrain
and water construct their own geometry, the cell and resource viewers consume
model-space meshes, and the only intentional world-space bake remains the
one-off conflict comparison helper.

## Why

`world_meshes` composes each shape's node-chain transform and **bakes it into the
vertices** (`world.apply(v)`), returning flat world-space triangle soup. That was
simple, but it flattens every model's *internal* node hierarchy into its
vertices, and that has cost us:

- **Animation is a workaround.** A `NiKeyframeController` animates a *node*, but
  there is no node left to animate — the transform is already in the vertices. The
  current transform animation reconstructs a per-frame matrix *delta*
  (`parent · L(t) · (parent · rest)⁻¹`) to move baked-at-rest vertices, which is
  indirection we only need because the node was baked away.
- **Particles/mist have nowhere to attach.** An emitter is a *node* with a
  position and a controller; with the hierarchy flattened there is no live node to
  spawn particles from or move.
- **It re-bakes per placement on one path.** `build_scene` copies and re-applies a
  placement transform to every vertex; only `build_instanced` avoids it.

The target keeps the one thing baking bought — **instancing** (a cell that places
one crate 300 times draws one geometry + 300 matrices, not 300 crates) — while
removing the flattening:

> **Model-space + per-shape instancing.** Geometry stays in each shape's own local
> space. Each shape's node-world transform (the composition of its node chain) is
> *folded into the per-instance matrix* rather than into the vertices, and is
> recomputed per frame only when a node on its chain animates. Node transforms
> compose in the render loop, exactly as a real scene graph does.

For a **static** shape the folded result is bit-identical to today's baked
position (`instance = placement · nodeWorld`, `vertices` local, vs. `placement`
applied to `nodeWorld`-baked vertices). Nothing on screen moves; the difference is
that `nodeWorld` is now *separable*, so it can animate, and the geometry is shared
across instances instead of pre-transformed.

## Invariants (must hold at every stage)

- **Static render is pixel-identical.** A cell with no animation looks exactly as
  it does now, at the same draw count. This is the gate on every stage.
- **Draw count unchanged.** One `InstancedMesh` per shape per model, as today.
- **One bad mesh never sinks a cell** (unchanged from now).
- **The default scan pays nothing.** `animation`/geometry retention stays opt-in;
  a mod-folder scan still walks counts, not values.

## Target representation

A parsed model becomes a list of *shape instances of one model*, each carrying:

| field | space | notes |
|---|---|---|
| `vertices`, `triangles`, `uvs`, colors | **shape-local** | straight from `NiTriShapeData`, un-transformed |
| textures, material, alpha, flags | — | as today |
| `node_world: Transform` | model (NIF-root) | rest composition of the shape's node chain, root-rotation dropped as today |
| `transform_anim` | — | now expressed as `(above, rest, keys)` where `nodeWorld(t) = above · L(t) · below`; **no delta, no inverse** |
| `uv_anim`, `vis_anim` | — | unchanged (texture/visibility, space-independent) |
| `emitter_node` (later) | model | the live node particles attach to |

The per-placement transform stays out of the geometry entirely: the viewer's
instance matrix is `placement · node_world` (static) or `placement · nodeWorld(t)`
(animated), computed where it belongs — in the loop.

## Stages (each independently shippable and green)

### Stage 1 — extraction, behind a wrapper (no behaviour change)

- Add `model_shapes(parsed) -> list[ShapeInstance]`: the walk, but it stops
  short of `world.apply(v)` — it keeps `vertices` local and records `node_world`
  (the composed chain) and the animation split per shape.
- Reimplement `world_meshes` as a thin wrapper that bakes:
  `Mesh(vertices=[s.node_world.apply(v) for v in s.vertices], …)`. **Every current
  caller is untouched** and byte-identical.
- Test: for a corpus of fixtures, `world_meshes` (new wrapper) equals
  `world_meshes` (old) vertex-for-vertex. This is the safety net for everything
  below.

### Stage 2 — the viewer consumes model space

- Serialise `node_world` (a 4×4) and local geometry instead of baked vertices.
- JS build: instance matrix `= placement · node_world` folded at build for static
  shapes; the InstancedMesh is otherwise identical.
- Replace the transform-animation *delta* with the direct form: an animated
  shape's per-frame instance matrix is `placement · above · L(t) · below`. The
  standalone (non-instanced) mesh sets its own `.matrix` the same way. Delete
  `invParentRest` and the delta math.
- Gate: static cells pixel-identical; animated content plays as now (or better).

### Stage 3 — migrate the remaining world-space consumers

Audit and move each off baked `world_meshes`, or leave it on the wrapper if that
is genuinely simplest:

- `bounds()` — compute from `node_world.apply` over shape corners; cheap, no full
  bake needed.
- **Conflict diff** — comparison is a one-off; it may keep the baking wrapper (a
  bake for comparison is fine and not a hot path). Decide per measurement.
- **Terrain / water** — these build their meshes directly (land grid, water quad),
  not from NIF `world_meshes`; confirm they are unaffected (expected: yes).
- `build_scene` — its per-placement re-bake folds into the instance-matrix model
  and likely disappears, or becomes a thin bake for any caller that still wants
  flat world-space.

### Stage 4 — retire baking from the hot path

Once the viewer and hot callers are on model space, the world-space bake survives
only as an on-demand helper (bounds, conflict) — not something every previewed
mesh pays. `world_meshes` either stays as that explicit helper or is renamed to
say so (`baked_world_meshes`), so no future code reaches for it by habit.

## Perf

- **Draw count:** unchanged (per-shape InstancedMesh).
- **Static matrices:** folded once at build (`placement · node_world`), as today.
- **Animated matrices:** recomputed per frame — only for shapes on an animated
  node's chain, a small fraction; a handful of `Matrix4` multiplies at 25 fps.
- **Memory:** geometry is shared across instances (local, uploaded once) instead
  of pre-transformed per model; strictly ≤ today.
- **On free-threaded 3.13+ (PEP 703):** the per-model parse now runs on a thread
  pool when the caller supplies multiple workers. Wraithguard's cell preview
  enables that path only for a GIL-free interpreter, so the normal GIL build
  does not pay thread-pool overhead. The approach is independent of the
  un-baking representation and is credited to Gardenfell in `CREDITS.md`.

### Asset-loading follow-up

The Gardenfell/GrassForge "read once and reuse" approach is now applied to the
viewer asset path in places that previously did repeat work:

- The merged mesh VFS and the texture resolver share the same opened BSA index,
  so the archive tables are parsed once rather than once per subsystem.
- Texture resolution and auxiliary-map discovery are cached, and decoded texture
  results are keyed by the actual provider rather than the spelling of the mesh
  reference.
- The cell viewer probes DDS headers without reading the mip payload. DXT1/3/5
  textures are then served as their original compressed blocks and uploaded via
  `THREE.CompressedTexture`, avoiding Python DDS decode plus PNG encode entirely.
- Compressed texture payloads are cached by source, so a later cell preview can
  reuse the same blocks instead of rereading the archive or loose file.
- Served cell previews now send packed geometry and instance matrices as loopback
  binary blobs instead of base64-inlining them in the HTML. Standalone exports keep
  the inline path, while the in-app/server path gets a small document and parallel
  binary fetches.

These changes target Wraithguard's actual loading costs without copying
Gardenfell's Rust/GPU implementation. There is not a controlled same-corpus,
same-hardware benchmark in this checkout, so the migration deliberately does not
claim numerical parity with Gardenfell.

## Risks and how each is contained

- **Breaking terrain/water/conflict.** Stage 1's wrapper keeps them byte-identical
  until each is deliberately migrated in Stage 3, one at a time, behind the
  equality test.
- **A subtle transform error in the fold.** The Stage-1 equality test catches any
  divergence between baked and folded positions before the viewer ever changes.
- **Instancing an animated model.** Per-shape instance matrices recomputed per
  frame is already proven by the current transform-animation path; Stage 2 only
  makes it the direct composition instead of a delta.

## Not in scope here

KF-file binding and the particle system are *downstream* of this: both become
straightforward once a live node hierarchy exists (a KF track drives a node; an
emitter is a node). They are separate tasks, unblocked by Stage 2.
