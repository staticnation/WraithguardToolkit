"""Turning a parsed NIF into triangles positioned in world space.

:mod:`mlox_subset.nif.reader` reads blocks; it does not assemble them. A mesh
is a *tree*: every shape carries its own transform, every node above it carries
another, and a shape's vertices mean nothing until those are composed down the
chain. A viewer that skipped that would draw every part of a mesh piled on the
origin.

This module does the composition and nothing else. It takes a file parsed with
``geometry=True`` and returns flat triangle soup with world-space coordinates,
which is the form both a renderer and a bounding-box calculation want.

**Roots are found, not assumed.** Block 0 is conventionally the root, and
conventionally is not good enough for files written by twenty years of
exporters. A root here is any node no other node claims as a child, which is
derived from the child links rather than trusted.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from mlox_subset.logging_setup import get_logger
from mlox_subset.nif.report import COLLISION_NODES as COLLISION_HINT, normalise_texture

if TYPE_CHECKING:
    from mlox_subset.nif.reader import Block, NifFile

LOG = get_logger(__name__)

#: Block types that hold children and therefore continue the walk.
_NODE_TYPES: frozenset[str] = frozenset(
    {
        "NiNode",
        "RootCollisionNode",
        "AvoidNode",
        "NiBSParticleNode",
        "NiBSAnimationNode",
        "NiBillboardNode",
        "NiSwitchNode",
        "NiLODNode",
    }
)

#: Block types that carry drawable geometry.
_SHAPE_TYPES: frozenset[str] = frozenset({"NiTriShape"})

#: How deep the walk may go before it concludes the graph has a cycle. NIF
#: children are links by index and nothing in the format forbids a loop, so a
#: hostile or broken file could otherwise spin forever.
_MAX_DEPTH: int = 64


@dataclass(frozen=True, slots=True)
class Transform:
    """A rotation, a uniform scale and a translation, applied in that order.

    Attributes:
        rotation: Three rows of three, as the file stores it.
        scale: Uniform scale factor.
        translation: Offset applied after rotating and scaling.
    """

    rotation: tuple[tuple[float, float, float], ...] = (
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 0.0, 1.0),
    )
    scale: float = 1.0
    translation: tuple[float, float, float] = (0.0, 0.0, 0.0)

    def apply(self, point: tuple[float, float, float]) -> tuple[float, float, float]:
        """Move one point into this transform's frame.

        Args:
            point: The point to move.

        Returns:
            The transformed point.
        """
        x, y, z = point
        rows = self.rotation
        return (
            (rows[0][0] * x + rows[0][1] * y + rows[0][2] * z) * self.scale + self.translation[0],
            (rows[1][0] * x + rows[1][1] * y + rows[1][2] * z) * self.scale + self.translation[1],
            (rows[2][0] * x + rows[2][1] * y + rows[2][2] * z) * self.scale + self.translation[2],
        )

    def then(self, child: Transform) -> Transform:
        """Compose a child's transform with this one.

        The child's rotation and scale are expressed in *this* frame, so the
        child's translation has to be carried through this transform rather
        than simply added -- adding them is the mistake that leaves a rotated
        parent's children in the wrong place.

        Args:
            child: The transform below this one in the tree.

        Returns:
            The combined transform.
        """
        rows = self.rotation
        other = child.rotation
        combined: tuple[tuple[float, float, float], ...] = tuple(
            (
                sum(rows[i][k] * other[k][0] for k in range(3)),
                sum(rows[i][k] * other[k][1] for k in range(3)),
                sum(rows[i][k] * other[k][2] for k in range(3)),
            )
            for i in range(3)
        )
        return Transform(
            rotation=combined,
            scale=self.scale * child.scale,
            translation=self.apply(child.translation),
        )


@dataclass(frozen=True, slots=True)
class Mesh:
    """One drawable shape, in world coordinates.

    Attributes:
        name: The shape's name, as the exporter wrote it.
        vertices: World-space positions.
        triangles: Index triples into :attr:`vertices`.
        uvs: Texture coordinates, one per vertex, empty when the shape has
            none. Only the first UV set is kept: Morrowind draws from it, and
            the rest exist for tools rather than for the game.
        texture: The base texture path, normalised, or ``""`` when untextured.
    """

    name: str
    vertices: list[tuple[float, float, float]] = field(default_factory=list)
    triangles: list[tuple[int, int, int]] = field(default_factory=list)
    uvs: list[tuple[float, float]] = field(default_factory=list)
    texture: str = ""


def _transform_of(block: Block) -> Transform:
    """Read a block's own transform.

    Args:
        block: A scene object.

    Returns:
        Its transform, defaulting to identity when the fields are absent --
        which happens for a block read without ``geometry=True``.
    """
    rotation = block.fields.get("rotation_m3")
    translation = block.fields.get("translation_xyz")
    scale = block.fields.get("scale", 1.0)
    rows: tuple[tuple[float, float, float], ...] = (
        tuple((float(r[0]), float(r[1]), float(r[2])) for r in rotation)
        if rotation
        else Transform().rotation
    )
    return Transform(
        rotation=rows,
        scale=float(scale) if isinstance(scale, (int, float)) else 1.0,
        translation=(
            (float(translation[0]), float(translation[1]), float(translation[2]))
            if translation
            else (0.0, 0.0, 0.0)
        ),
    )


def find_roots(parsed: NifFile) -> list[int]:
    """Find the blocks that nothing claims as a child.

    Args:
        parsed: A parsed file.

    Returns:
        Root block indices, in file order.
    """
    claimed: set[int] = set()
    for block in parsed.blocks:
        for child in block.fields.get("children_links") or []:
            if child >= 0:
                claimed.add(int(child))
    return [b.index for b in parsed.blocks if b.index not in claimed]


def world_meshes(parsed: NifFile) -> list[Mesh]:
    """Collect every drawable shape, with vertices in world space.

    Args:
        parsed: A file parsed with ``geometry=True``. Parsed without it the
            vertex arrays are absent and the result is empty, which is honest:
            the data was never read.

    Returns:
        One :class:`Mesh` per shape that has geometry.
    """
    by_index = {block.index: block for block in parsed.blocks}
    meshes: list[Mesh] = []
    seen: set[int] = set()

    def walk(index: int, parent: Transform, depth: int) -> None:
        block = by_index.get(index)
        if block is None or depth > _MAX_DEPTH or index in seen:
            if depth > _MAX_DEPTH:
                LOG.warning("scene graph deeper than %d at block %d; stopping", _MAX_DEPTH, index)
            return
        seen.add(index)
        here = parent.then(_transform_of(block))
        if block.type_name in _SHAPE_TYPES:
            mesh = _shape_to_mesh(block, here, by_index)
            if mesh is not None:
                meshes.append(mesh)
        if block.type_name in _NODE_TYPES:
            for child in block.fields.get("children_links") or []:
                if child >= 0:
                    walk(int(child), here, depth + 1)

    for root in find_roots(parsed):
        walk(root, Transform(), 0)
    return meshes


def _shape_to_mesh(block: Block, world: Transform, by_index: dict[int, Block]) -> Mesh | None:
    """Build one mesh from a shape and its data block.

    Args:
        block: The ``NiTriShape``.
        world: Its composed world transform.
        by_index: Every parsed block, by index.

    Returns:
        The mesh, or ``None`` when its data block was not reached or carries no
        retained vertices.
    """
    data = by_index.get(block.link("data"))
    if data is None:
        return None
    vertices = data.fields.get("vertices_xyz")
    triangles = data.fields.get("triangles_indices")
    if not vertices or not triangles:
        return None
    # A shape can declare several UV sets. Only the first is Morrowind's, and
    # taking more would mean guessing which one a texture belongs to.
    uvs = (data.fields.get("uv_sets_uv") or [])[: len(vertices)]
    return Mesh(
        name=_name_of(block),
        vertices=[world.apply(v) for v in vertices],
        triangles=list(triangles),
        uvs=[(float(u), float(v)) for u, v in uvs],
        texture=_base_texture(block, by_index),
    )


def _base_texture(block: Block, by_index: dict[int, Block]) -> str:
    """Find the base texture a shape draws with.

    Args:
        block: The shape.
        by_index: Every parsed block, by index.

    Returns:
        The normalised texture path, or ``""``.
    """
    for prop_index in block.fields.get("properties_links") or []:
        prop = by_index.get(int(prop_index))
        if prop is None or prop.type_name != "NiTexturingProperty":
            continue
        slots = prop.fields.get("textures")
        if not isinstance(slots, dict):
            continue
        source = by_index.get(int(slots.get("base", -1)))
        if source is None:
            continue
        reference = source.fields.get("external_or_internal")
        if isinstance(reference, str) and reference.strip():
            return normalise_texture(reference)
    return ""


@dataclass(frozen=True, slots=True)
class TreeNode:
    """One entry in a mesh's block hierarchy.

    Attributes:
        index: The block index, so a reader can match it against a survey.
        type_name: The block's type.
        name: Its name, empty for blocks that have none.
        note: A short summary of what it carries, when that is worth saying.
        children: Nested entries.
    """

    index: int
    type_name: str
    name: str = ""
    note: str = ""
    children: list[TreeNode] = field(default_factory=list)


#: Blocks worth listing under their parent even though they are not children in
#: the scene-graph sense. These are exactly the things the 3D view cannot show:
#: a property is not geometry, a controller draws nothing, and a collision node
#: is invisible in a render but decides whether you can walk through the thing.
_ATTACHMENT_LINKS: tuple[str, ...] = ("properties_links", "data", "controller", "skin_instance")


def _name_of(block: Block) -> str:
    """A block's name, or empty when it has none.

    Args:
        block: Any block.

    Returns:
        The name as text.
    """
    name = block.fields.get("name")
    return name if isinstance(name, str) else ""


def block_tree(parsed: NifFile) -> list[TreeNode]:
    """Describe a file's block hierarchy.

    A structural companion to :func:`world_meshes`: that returns what can be
    drawn, this returns everything, including the blocks that never appear in
    a render and are often the reason a conflict matters.

    Args:
        parsed: A parsed file. Child links need ``geometry=True``; without them
            the result is a flat list of roots, which is honest rather than
            wrong.

    Returns:
        One entry per root, with children nested.
    """
    by_index = {block.index: block for block in parsed.blocks}
    seen: set[int] = set()

    def describe(block: Block) -> str:
        if block.type_name == "NiTriShapeData":
            return f"{block.fields.get('num_vertices', 0)} verts, {block.fields.get('num_triangles', 0)} tris"
        if block.type_name == "NiSourceTexture":
            reference = block.fields.get("external_or_internal")
            return normalise_texture(reference) if isinstance(reference, str) else "embedded"
        if block.type_name in COLLISION_HINT:
            return "collision"
        if block.type_name.endswith("Controller"):
            return "animated"
        if block.type_name == "NiSkinInstance":
            return "skinned"
        return ""

    def build(index: int, depth: int) -> TreeNode | None:
        block = by_index.get(index)
        if block is None or index in seen or depth > _MAX_DEPTH:
            return None
        seen.add(index)
        node = TreeNode(
            index=block.index,
            type_name=block.type_name,
            name=_name_of(block),
            note=describe(block),
        )
        for key in _ATTACHMENT_LINKS:
            value = block.fields.get(key)
            targets = value if isinstance(value, list) else [value]
            for target in targets:
                if isinstance(target, int) and target >= 0:
                    child = build(int(target), depth + 1)
                    if child is not None:
                        node.children.append(child)
        for child_index in block.fields.get("children_links") or []:
            if child_index >= 0:
                child = build(int(child_index), depth + 1)
                if child is not None:
                    node.children.append(child)
        return node

    roots = [build(index, 0) for index in find_roots(parsed)]
    trees = [node for node in roots if node is not None]
    # Anything the walk never reached is still in the file and still worth
    # showing -- an orphaned block is exactly the sort of thing a person
    # opening this view wants to know about.
    trees.extend(
        TreeNode(b.index, b.type_name, _name_of(b), describe(b))
        for b in parsed.blocks
        if b.index not in seen
    )
    return trees


def bounds(meshes: list[Mesh]) -> tuple[tuple[float, float, float], tuple[float, float, float]]:
    """Axis-aligned bounds of a whole mesh set.

    A viewer needs this before it can frame anything, and it is cheap once the
    vertices are already in world space.

    Args:
        meshes: The meshes to measure.

    Returns:
        Minimum and maximum corners. Both are the origin when there is nothing
        to measure, so a caller never has to special-case an empty file.
    """
    points = [v for mesh in meshes for v in mesh.vertices]
    if not points:
        return ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0))
    return (
        (min(p[0] for p in points), min(p[1] for p in points), min(p[2] for p in points)),
        (max(p[0] for p in points), max(p[1] for p in points), max(p[2] for p in points)),
    )
