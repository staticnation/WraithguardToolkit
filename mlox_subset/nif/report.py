"""Turning parsed blocks into the questions a resource conflict raises.

When two mods ship the same mesh path, the list says one wins. What it does not
say is whether that matters, and the answers that decide it are all structural:

* **Is the winner far simpler?** A retexture that also replaces the mesh with a
  low-poly stand-in is a downgrade nobody asked for, and triangle counts show it
  immediately.
* **Does it ask for textures the other does not ship?** A mesh referencing
  ``tx_new_rock.dds`` when only the other provider has that file is a missing
  texture in game -- and the file that breaks is not the one that lost.
* **Did it lose collision?** ``RootCollisionNode`` present in one and absent in
  the other means the winner is walk-through, which is the kind of thing found
  by falling out of the world rather than by reading a list.
* **Did it lose animation?** A door or banner whose winning mesh has no
  controllers simply stops moving.

None of that needs geometry, materials or a renderer, which is why this reads a
structure rather than drawing anything.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from mlox_subset.nif.reader import Block, NifFile

#: Block types that exist to carry collision geometry. Presence is the whole
#: signal: Morrowind has no collision flag, it has a node whose name says so.
COLLISION_NODES: frozenset[str] = frozenset({"RootCollisionNode"})

#: Anything deriving from NiTimeController. A mesh with one of these moves.
CONTROLLER_SUFFIX = "Controller"


@dataclass(frozen=True, slots=True)
class Shape:
    """One drawable piece of a mesh.

    Attributes:
        name: The shape's own name, as the exporter wrote it.
        vertices: Vertex count.
        triangles: Triangle count.
    """

    name: str
    vertices: int
    triangles: int


@dataclass(frozen=True, slots=True)
class Structure:
    """What a mesh contains, in the terms a conflict is judged on.

    Attributes:
        shapes: Every ``NiTriShape``, with its counts.
        textures: External texture paths the mesh references, lower-cased and
            slash-normalised so two mods spelling the same path differently
            compare equal.
        has_collision: Whether a collision node is present.
        has_animation: Whether any controller is present.
        node_count: How many scene nodes there are.
        blocks_read: How many blocks were parsed.
        blocks_declared: How many the header said there were.
        stopped_reason: Why parsing stopped, empty when it did not.
    """

    shapes: list[Shape] = field(default_factory=list)
    textures: list[str] = field(default_factory=list)
    has_collision: bool = False
    has_animation: bool = False
    node_count: int = 0
    blocks_read: int = 0
    blocks_declared: int = 0
    stopped_reason: str = ""

    @property
    def total_vertices(self) -> int:
        """Vertices across every shape."""
        return sum(shape.vertices for shape in self.shapes)

    @property
    def total_triangles(self) -> int:
        """Triangles across every shape."""
        return sum(shape.triangles for shape in self.shapes)

    @property
    def partial(self) -> bool:
        """Whether the file was only partly read.

        A partial structure can still answer "does it have collision" in the
        affirmative, but never in the negative -- the node may simply be in the
        part that was not reached. Callers that draw a conclusion from an
        absence have to check this first.
        """
        return bool(self.stopped_reason) or self.blocks_read != self.blocks_declared


def normalise_texture(path: str) -> str:
    """Reduce a texture reference to a comparable form.

    Morrowind asset paths are case-insensitive and written with either slash,
    and mods are inconsistent about both. Comparing them raw reports two
    spellings of one path as two different textures.

    Args:
        path: The reference as the mesh stores it.

    Returns:
        The path lower-cased with backslashes turned to forward slashes and
        surrounding whitespace removed.
    """
    return path.strip().replace("\\", "/").lower()


def texture_key(path: str) -> str:
    """Reduce a texture reference to what actually identifies the file.

    **The extension is not part of a texture's identity.** Morrowind resolves a
    reference by path and name and loads whatever is there: a mesh saying
    ``darkbrotherhood_head.bmp`` draws with ``darkbrotherhood_head.dds`` if
    that is what shipped, and base-game meshes routinely name ``.bmp`` or
    ``.tga`` for files that only exist as ``.dds``.

    Comparing references verbatim therefore invents differences. Two versions
    of a mesh naming the same texture with different extensions were reported
    as one *adding* a texture and the other *dropping* it -- a finding about
    nothing, on the line a user is most likely to act on.

    The ``textures/`` prefix is dropped for the same reason: some exporters
    write it, some do not, and it names the same file either way.

    Args:
        path: The reference as the mesh stores it.

    Returns:
        A comparison key. Not for display -- the reference as written is more
        useful to a person, and is what the report shows.
    """
    key = normalise_texture(path).lstrip("/")
    while "//" in key:
        key = key.replace("//", "/")
    if key.startswith("textures/"):
        key = key[len("textures/") :]
    return key.rsplit(".", 1)[0] if "." in key.rsplit("/", 1)[-1] else key


def summarise(parsed: NifFile) -> Structure:
    """Reduce parsed blocks to a structure summary.

    Args:
        parsed: The result of reading one file.

    Returns:
        What the mesh contains.
    """
    by_index = {block.index: block for block in parsed.blocks}
    shapes = [
        _shape_of(block, by_index) for block in parsed.blocks if block.type_name == "NiTriShape"
    ]
    textures: list[str] = []
    for block in parsed.blocks:
        if block.type_name != "NiSourceTexture":
            continue
        reference = block.fields.get("external_or_internal")
        if isinstance(reference, str) and reference.strip():
            normalised = normalise_texture(reference)
            if normalised not in textures:
                textures.append(normalised)
    return Structure(
        shapes=shapes,
        textures=textures,
        has_collision=any(b.type_name in COLLISION_NODES for b in parsed.blocks),
        has_animation=any(b.type_name.endswith(CONTROLLER_SUFFIX) for b in parsed.blocks),
        node_count=sum(1 for b in parsed.blocks if b.type_name.endswith("Node")),
        blocks_read=len(parsed.blocks),
        blocks_declared=parsed.block_count,
        stopped_reason=parsed.stopped_reason,
    )


def _shape_of(block: Block, by_index: dict[int, Block]) -> Shape:
    """Pair a shape with the counts held in its data block.

    Args:
        block: The ``NiTriShape``.
        by_index: Every parsed block, by index.

    Returns:
        The shape. Counts are zero when its data block was not reached, which
        is honest rather than convenient: a shape whose data is missing has an
        unknown size, not an empty one.
    """
    data = by_index.get(block.link("data"))
    vertices = triangles = 0
    if data is not None and data.type_name == "NiTriShapeData":
        vertices = int(data.fields.get("num_vertices") or 0)
        triangles = int(data.fields.get("triangles") or 0)
    name = block.fields.get("name")
    return Shape(name if isinstance(name, str) else "", vertices, triangles)


@dataclass(frozen=True, slots=True)
class Difference:
    """How the winning mesh differs from the one it overrides.

    Attributes:
        triangle_ratio: The winner's triangles over the loser's. ``None`` when
            the loser has none, since a ratio against zero says nothing.
        lost_collision: The loser had collision and the winner does not.
        lost_animation: The loser had controllers and the winner does not.
        added_textures: Texture paths only the winner asks for.
        dropped_textures: Texture paths only the loser asked for.
        unreliable: Either side was only partly read, so every *absence* here
            is unproven.
    """

    triangle_ratio: float | None
    lost_collision: bool
    lost_animation: bool
    added_textures: list[str]
    dropped_textures: list[str]
    unreliable: bool


def compare(loser: Structure, winner: Structure) -> Difference:
    """Describe what changes when the winner replaces the loser.

    Only losses are named. A winner that *adds* collision or animation is
    reported through the absence of the corresponding flag rather than as its
    own finding: gaining detail is rarely the thing that breaks a load order,
    and a report that lists every difference equally is one nobody reads.

    Args:
        loser: The overridden mesh.
        winner: The mesh that wins the VFS.

    Returns:
        The differences worth a person's attention.
    """
    ratio = None
    if loser.total_triangles:
        ratio = winner.total_triangles / loser.total_triangles
    # Compared by identity, reported as written. See :func:`texture_key`: the
    # extension is not part of what a reference names.
    loser_keys = {texture_key(t) for t in loser.textures}
    winner_keys = {texture_key(t) for t in winner.textures}
    return Difference(
        triangle_ratio=ratio,
        lost_collision=loser.has_collision and not winner.has_collision,
        lost_animation=loser.has_animation and not winner.has_animation,
        added_textures=[t for t in winner.textures if texture_key(t) not in loser_keys],
        dropped_textures=[t for t in loser.textures if texture_key(t) not in winner_keys],
        unreliable=loser.partial or winner.partial,
    )
