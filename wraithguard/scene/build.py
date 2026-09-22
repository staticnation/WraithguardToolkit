"""Assemble a cell's placements into geometry the mesh viewer can draw.

Two strategies, both here:

* :func:`build_scene` *bakes* each placement's transform into a copy of its
  model's world-space meshes -- one flat mesh list the standalone viewer renders
  like a single NIF. Simple, but a copy per placement: fine for a single item,
  ruinous for a cell that reuses a crate three hundred times.
* :func:`build_instanced` groups placements by model -- each unique model loaded
  once, plus a matrix per placement (:func:`matrix4_columns`) -- for
  ``THREE.InstancedMesh``. One crate mesh and three hundred matrices instead of
  three hundred crates: the difference between a cell that opens and one that
  exhausts memory. This is the path the cell viewer uses.

Mesh loading is injected as a callable, so this stays pure and unit-tested: the
caller supplies a loader that resolves a model path across the data folders (see
:func:`wraithguard.nif.vfs.read_mesh`) and returns its
:func:`~wraithguard.nif.geometry.world_meshes`, or ``None`` when it cannot be
read -- one unreadable mesh must not sink the whole cell.
"""

from __future__ import annotations

from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING

from wraithguard.nif.geometry import bake_mesh

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence

    from wraithguard.nif.geometry import Mesh, Transform
    from wraithguard.scene.resolve import Placement

#: A model loader: model path (e.g. ``"f/flora_tree_01.nif"``) -> its world
#: meshes, or ``None`` when the mesh cannot be found or read.
MeshLoader = "Callable[[str], list[Mesh] | None]"


@dataclass
class BuiltScene:
    """A cell assembled for the viewer.

    Attributes:
        meshes: Every placed object's meshes, transformed into cell space and
            ready for :func:`~wraithguard.nif.viewer.build_viewer_page`.
        drawn: How many placements contributed geometry.
        missing_models: Model paths that could not be loaded, de-duplicated in
            first-seen order -- the "N meshes not found" line.
    """

    meshes: list[Mesh] = field(default_factory=list)
    drawn: int = 0
    missing_models: list[str] = field(default_factory=list)


def build_scene(
    placements: Sequence[Placement],
    load_mesh: Callable[[str], list[Mesh] | None],
) -> BuiltScene:
    """Bake every placed reference into world-space meshes for the viewer.

    Each model is loaded at most once (repeats are common -- a cell reuses the
    same rock or crate many times) and re-baked per placement. Non-drawable
    placements (``no_mesh_record``/``editor_marker``/``actor``) are skipped.

    Args:
        placements: The cell's placements, from
            :func:`wraithguard.scene.resolve.resolve_cell`.
        load_mesh: Resolves a model path to its world meshes, or ``None``.

    Returns:
        A :class:`BuiltScene`.
    """
    cache: dict[str, list[Mesh] | None] = {}
    out: list[Mesh] = []
    drawn = 0
    missing: list[str] = []
    for placement in placements:
        if placement.kind != "placed" or not placement.model:
            continue
        if placement.model not in cache:
            cache[placement.model] = load_mesh(placement.model)
        base = cache[placement.model]
        if base is None:
            if placement.model not in missing:
                missing.append(placement.model)
            continue
        transform = placement.transform
        for mesh in base:
            # This flat path bakes to world space: fold the shape's own node
            # transform in first (a no-op on an already-baked mesh), then the
            # placement. A node animation's delta is built from parent/rest, so
            # prefix the placement onto ``parent`` to keep it correct in the baked
            # frame -- the instanced path leaves vertices in model space instead.
            baked = bake_mesh(mesh)
            out.append(
                replace(
                    baked,
                    vertices=[transform.apply(v) for v in baked.vertices],
                    transform_anim=(
                        replace(
                            baked.transform_anim,
                            parent=transform.then(baked.transform_anim.parent),
                        )
                        if baked.transform_anim is not None
                        else None
                    ),
                )
            )
        drawn += 1
    return BuiltScene(meshes=out, drawn=drawn, missing_models=missing)


def matrix4_columns(transform: Transform) -> tuple[float, ...]:
    """A :class:`~wraithguard.nif.geometry.Transform` as a column-major 4x4.

    The 16 floats a ``THREE.Matrix4`` (and so ``InstancedMesh.setMatrixAt``)
    expects: rotation scaled into the upper-left 3x3, translation in the last
    column, laid out column by column. This is what lets a placed object be an
    *instance* of its model -- the model's vertices stay in model space and this
    matrix positions them -- instead of a baked copy with the transform applied
    to every vertex.

    Args:
        transform: The placement transform (see
            :func:`wraithguard.scene.resolve.reference_transform`).

    Returns:
        Sixteen floats, column-major.
    """
    r = transform.rotation
    s = transform.scale
    tx, ty, tz = transform.translation
    return (
        r[0][0] * s, r[1][0] * s, r[2][0] * s, 0.0,
        r[0][1] * s, r[1][1] * s, r[2][1] * s, 0.0,
        r[0][2] * s, r[1][2] * s, r[2][2] * s, 0.0,
        tx, ty, tz, 1.0,
    )  # fmt: skip


@dataclass
class InstancedGroup:
    """One model and every place it is instanced in the cell.

    Attributes:
        model: The model path (the group key).
        meshes: The model's meshes in *model space* -- loaded once, never baked;
            the viewer draws them once as a ``THREE.InstancedMesh``.
        matrices: One flat column-major 4x4 (16 floats) per placement, in
            placement order -- the per-instance world transforms.
        ref_ids: The object id each instance places, in the same order as
            ``matrices`` -- what a click on an instance resolves to (the CS
            "Object ID", e.g. ``ex_common_house_addon``).
    """

    model: str
    meshes: list[Mesh]
    matrices: list[float] = field(default_factory=list)
    ref_ids: list[str] = field(default_factory=list)
    record_type: str = ""
    """The friendly record type (Static, Light, ...) of this group's objects, for
    the viewer's per-type visibility toggles. Taken from the first placement --
    a model is essentially always one type."""
    focus: bool = False
    """Whether the camera should frame on this group. Set for terrain, which
    spans exactly the cell, so a stray reference far outside it cannot throw the
    view off -- the camera lands *in the cell*, not around every last object."""
    adjacent: bool = False
    """Whether this group belongs to a *neighbouring* cell rather than the one
    being previewed. Adjacent cells give the picked cell its context -- a merge
    seam or a floater reads against the ground next door -- but are drawn behind
    a toggle and never framed on, so the camera still lands in the picked cell."""
    skirt_alone: bool = False
    """Whether this group is the focused cell's *lone-cell* skirt: the plinth that
    wraps the picked cell when its neighbours are hidden. The viewer shows it only
    while the "Adjacent cells" toggle is off, handing over to the block-perimeter
    skirt (an ``adjacent`` group) when neighbours are shown, so the skirt always
    hugs the outer edge of whatever is on screen rather than walling off a shared
    seam."""

    @property
    def count(self) -> int:
        """How many instances (placements) this group holds."""
        return len(self.matrices) // 16


@dataclass
class InstancedCell:
    """A cell assembled for instanced drawing.

    Attributes:
        groups: One :class:`InstancedGroup` per unique model, in first-seen
            order.
        drawn: Total placements that contributed geometry (sum of instance
            counts).
        missing_models: Model paths that could not be loaded, de-duplicated in
            first-seen order.
    """

    groups: list[InstancedGroup] = field(default_factory=list)
    drawn: int = 0
    missing_models: list[str] = field(default_factory=list)


def _group_type(record_type: str, meshes: Sequence[Mesh]) -> str:
    """The category a group of instances should carry.

    The record tag (``Static``, ``Activator``, ...) is the default, but a model
    whose file is a particle emitter -- mist, fog, glowbugs -- is re-categorised
    as ``"Emitter"`` so the viewer can toggle all such effects at once. A
    ``Light`` is left alone even when it carries a flame particle: a torch is a
    light first, and the user reads it as one.

    Args:
        record_type: The friendly type from the placement's record tag.
        meshes: The model's loaded meshes (any carrying
            :attr:`~wraithguard.nif.geometry.Mesh.emitter` marks the file).

    Returns:
        ``"Emitter"`` for a non-light emitter file, else ``record_type``.
    """
    if record_type != "Light" and any(mesh.emitter for mesh in meshes):
        return "Emitter"
    return record_type


def build_instanced(
    placements: Sequence[Placement],
    load_mesh: Callable[[str], list[Mesh] | None],
    *,
    workers: int = 1,
) -> InstancedCell:
    """Group placements by model for instanced drawing (no vertex baking).

    Unlike :func:`build_scene`, which bakes each placement's transform into a
    copy of its model's vertices, this loads every unique model once and records
    the per-placement transforms as matrices. A cell that places the same crate
    three hundred times then carries one crate mesh and three hundred matrices,
    not three hundred crates -- the difference between a page that opens and one
    that exhausts memory.

    Args:
        placements: The cell's placements, from
            :func:`wraithguard.scene.resolve.resolve_cell`.
        load_mesh: Resolves a model path to its model-space meshes, or ``None``.
        workers: Threads to parse the unique models on. ``>1`` parses them
            concurrently up front (worth it only on a free-threaded interpreter,
            and ``load_mesh`` must then be thread-safe); ``1`` is the serial parse.

    Returns:
        An :class:`InstancedCell`.
    """
    cache: dict[str, list[Mesh] | None] = {}
    groups: dict[str, InstancedGroup] = {}
    order: list[str] = []
    drawn = 0
    missing: list[str] = []
    # Parsing a model's NIF is the single largest cost of a build and each model
    # is independent, so with more than one worker the unique models are parsed on
    # a thread pool up front. This only pays off on a free-threaded interpreter
    # (Gardenfell's start-up win, PEP 703); under the GIL the caller passes
    # workers=1 and this is the same serial parse as before. ``load_mesh`` must be
    # thread-safe when workers > 1 (the cell previewer's cache guards itself).
    if workers > 1:
        unique = list(dict.fromkeys(p.model for p in placements if p.kind == "placed" and p.model))
        if unique:
            from concurrent.futures import ThreadPoolExecutor

            with ThreadPoolExecutor(max_workers=min(workers, len(unique))) as pool:
                cache.update(zip(unique, pool.map(load_mesh, unique), strict=True))
    for placement in placements:
        if placement.kind != "placed" or not placement.model:
            continue
        model = placement.model
        if model not in cache:
            cache[model] = load_mesh(model)
        base = cache[model]
        if base is None:
            if model not in missing:
                missing.append(model)
            continue
        group = groups.get(model)
        if group is None:
            group = InstancedGroup(
                model=model,
                meshes=list(base),
                record_type=_group_type(placement.record_type, base),
            )
            groups[model] = group
            order.append(model)
        group.matrices.extend(matrix4_columns(placement.transform))
        group.ref_ids.append(placement.ref_id)
        drawn += 1
    return InstancedCell(groups=[groups[m] for m in order], drawn=drawn, missing_models=missing)
