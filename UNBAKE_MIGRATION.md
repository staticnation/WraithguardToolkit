# Un-baking migration: model-space geometry + per-shape instancing

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
- **On free-threaded 3.13+ (PEP 703):** with the hierarchy no longer baked, the
  per-model parse is still the build's cost — a separate follow-up can parse the
  mesh superset on a thread pool (Gardenfell's start-up win), which the GIL no
  longer blocks. Independent of this migration; noted so it is not forgotten.

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
