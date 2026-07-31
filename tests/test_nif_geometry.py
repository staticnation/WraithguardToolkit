"""Tests for geometry retention and world-space composition.

Two things are being pinned down. That turning retention *on* adds data without
changing anything that was already there -- the scan path reads tens of
thousands of meshes and must not pay for a viewer feature. And that the
transform composition is actually composition, not addition, because the
difference only shows up once a parent is rotated and is invisible in the
identity-transform meshes that make up most of any corpus.
"""

from __future__ import annotations

import math
import struct

from wraithguard.nif.geometry import (
    Mesh,
    Transform,
    block_tree,
    bounds,
    find_roots,
    world_meshes,
)
from wraithguard.nif.reader import read_nif_bytes

HEADER = b"NetImmerse File Format, Version 4.0.0.2\n"


def text(value: str) -> bytes:
    """Length-prefixed, unterminated string.

    Args:
        value: The text.

    Returns:
        The encoded bytes.
    """
    raw = value.encode("cp1252")
    return struct.pack("<I", len(raw)) + raw


def nif(*blocks: tuple[str, bytes]) -> bytes:
    """Assemble a file from block bodies.

    Args:
        blocks: ``(type name, body)`` pairs in order.

    Returns:
        The file bytes.
    """
    out = [HEADER, struct.pack("<II", 0x04000002, len(blocks))]
    for type_name, body in blocks:
        out.append(text(type_name))
        out.append(body)
    return b"".join(out)


def av_body(
    name: str,
    *,
    translation: tuple[float, float, float] = (0.0, 0.0, 0.0),
    rotation: tuple[float, ...] = (1, 0, 0, 0, 1, 0, 0, 0, 1),
    scale: float = 1.0,
    children: tuple[int, ...] = (),
    properties: tuple[int, ...] = (),
    tail: bytes = b"",
) -> bytes:
    """A scene-object body, optionally with children.

    Args:
        name: The object's name.
        translation: Its translation.
        rotation: Its 3x3 rotation, row major.
        scale: Its scale.
        children: Child block indices; makes this a node.
        properties: Property block indices.
        tail: Extra bytes after the node lists.

    Returns:
        The body bytes.
    """
    body = text(name) + struct.pack("<iiH", -1, -1, 0)
    body += struct.pack("<3f", *translation)
    body += struct.pack("<9f", *rotation)
    body += struct.pack("<f", scale)
    body += struct.pack("<3f", 0.0, 0.0, 0.0)
    body += struct.pack("<I", len(properties)) + b"".join(struct.pack("<i", p) for p in properties)
    body += struct.pack("<I", 0)
    if children or not tail:
        body += struct.pack("<I", len(children)) + b"".join(struct.pack("<i", c) for c in children)
        body += struct.pack("<I", 0)
    return body + tail


def shape_data(
    vertices: list[tuple[float, float, float]], triangles: list[tuple[int, int, int]]
) -> bytes:
    """A ``NiTriShapeData`` body carrying real geometry.

    Args:
        vertices: The vertex positions.
        triangles: Index triples.

    Returns:
        The body bytes.
    """
    body = struct.pack("<H", len(vertices)) + struct.pack("<I", 1)
    body += b"".join(struct.pack("<3f", *v) for v in vertices)
    body += struct.pack("<I", 0)
    body += struct.pack("<4f", 0.0, 0.0, 0.0, 1.0)
    body += struct.pack("<I", 0)
    body += struct.pack("<HI", 0, 0)
    body += struct.pack("<HI", len(triangles), len(triangles) * 3)
    body += b"".join(struct.pack("<3H", *t) for t in triangles)
    return body + struct.pack("<H", 0)


SQUARE = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (1.0, 1.0, 0.0)]
ONE_TRIANGLE = [(0, 1, 2)]


class TestRetentionAddsWithoutChanging:
    """Turning geometry on must not alter what was already reported."""

    def test_the_counts_stay_where_they_were(self) -> None:
        """A structure report goes on reading ``vertices``; a viewer reads ``vertices_xyz``."""
        data = nif(("NiTriShapeData", shape_data(SQUARE, ONE_TRIANGLE)))
        plain = read_nif_bytes(data).blocks[0].fields
        rich = read_nif_bytes(data, geometry=True).blocks[0].fields
        assert plain["vertices"] == rich["vertices"]
        assert plain["num_vertices"] == rich["num_vertices"]
        assert all(rich[key] == value for key, value in plain.items())

    def test_the_default_path_retains_nothing(self) -> None:
        """Otherwise every scan would carry geometry it never asked for."""
        fields = (
            read_nif_bytes(nif(("NiTriShapeData", shape_data(SQUARE, ONE_TRIANGLE))))
            .blocks[0]
            .fields
        )
        assert not [key for key in fields if key.endswith(("_xyz", "_indices", "_links"))]

    def test_coordinates_come_back_as_written(self) -> None:
        """The whole point: real positions, not a byte count."""
        block = read_nif_bytes(
            nif(("NiTriShapeData", shape_data(SQUARE, ONE_TRIANGLE))), geometry=True
        ).blocks[0]
        assert block.fields["vertices_xyz"] == SQUARE
        assert block.fields["triangles_indices"] == ONE_TRIANGLE


class TestTransformCompositionIsNotAddition:
    """The failure mode that identity-transform test data cannot reveal."""

    def test_a_rotated_parent_carries_its_child_offset(self) -> None:
        """Adding translations instead of composing puts children in the wrong place.

        A parent rotated 90 degrees about Z with a child offset along +X must
        put the child along +Y. Summing the offsets would leave it on +X, and
        every mesh with an unrotated parent -- almost all of them -- would still
        look right.
        """
        quarter_turn = Transform(rotation=((0.0, -1.0, 0.0), (1.0, 0.0, 0.0), (0.0, 0.0, 1.0)))
        child = Transform(translation=(2.0, 0.0, 0.0))
        combined = quarter_turn.then(child)
        x, y, z = combined.apply((0.0, 0.0, 0.0))
        assert math.isclose(x, 0.0, abs_tol=1e-6), (x, y, z)
        assert math.isclose(y, 2.0, abs_tol=1e-6), (x, y, z)

    def test_scale_multiplies_down_the_chain(self) -> None:
        """Two halvings make a quarter, not a half."""
        half = Transform(scale=0.5)
        combined = half.then(Transform(scale=0.5))
        assert math.isclose(combined.scale, 0.25)

    def test_identity_composes_to_identity(self) -> None:
        """A negative control, so the maths above is not merely busy."""
        combined = Transform().then(Transform())
        assert combined.apply((3.0, -4.0, 5.0)) == (3.0, -4.0, 5.0)


class TestWorldPlacement:
    """Shapes must land where their parents put them."""

    def test_a_shape_is_offset_by_its_parent_node(self) -> None:
        """The reason composition exists at all."""
        parsed = read_nif_bytes(
            nif(
                ("NiNode", av_body("root", translation=(10.0, 0.0, 0.0), children=(1,))),
                ("NiTriShape", av_body("shape", tail=struct.pack("<ii", 2, -1))),
                ("NiTriShapeData", shape_data(SQUARE, ONE_TRIANGLE)),
            ),
            geometry=True,
        )
        meshes = world_meshes(parsed)
        assert len(meshes) == 1, parsed.stopped_reason
        assert meshes[0].vertices[0][0] == 10.0

    def test_roots_are_derived_not_assumed(self) -> None:
        """Block 0 is conventionally the root; convention is not evidence.

        Here block 0 is a *child* of block 1, so treating index 0 as the root
        would walk the tree from halfway down.
        """
        parsed = read_nif_bytes(
            nif(
                ("NiNode", av_body("child")),
                ("NiNode", av_body("parent", children=(0,))),
            ),
            geometry=True,
        )
        assert find_roots(parsed) == [1]

    def test_a_file_without_geometry_yields_no_meshes(self) -> None:
        """Honest emptiness: the data was never read, so there is none."""
        parsed = read_nif_bytes(
            nif(
                ("NiNode", av_body("root", children=(1,))),
                ("NiTriShape", av_body("shape", tail=struct.pack("<ii", 2, -1))),
                ("NiTriShapeData", shape_data(SQUARE, ONE_TRIANGLE)),
            )
        )
        assert world_meshes(parsed) == []

    def test_a_cycle_does_not_hang(self) -> None:
        """Nothing in the format forbids a child link pointing back up.

        The files come from mod archives, so "no sane exporter would" is not a
        guarantee about the input.
        """
        parsed = read_nif_bytes(
            nif(
                ("NiNode", av_body("a", children=(1,))),
                ("NiNode", av_body("b", children=(0,))),
            ),
            geometry=True,
        )
        assert world_meshes(parsed) == []


class TestBounds:
    """A viewer needs to frame the thing before it can show it."""

    def test_bounds_span_every_vertex(self) -> None:
        """Across meshes, not just within one."""
        low, high = bounds(
            [
                Mesh("a", [(0.0, 0.0, 0.0), (1.0, 2.0, 3.0)]),
                Mesh("b", [(-5.0, 0.0, 0.0)]),
            ]
        )
        assert low == (-5.0, 0.0, 0.0)
        assert high == (1.0, 2.0, 3.0)

    def test_empty_bounds_are_the_origin_not_an_error(self) -> None:
        """So a caller never has to special-case a mesh it could not read."""
        assert bounds([]) == ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0))


class TestBlockTree:
    """The structure pane's data: everything, including what never draws."""

    def _parsed(self):
        """A file with a shape, its data, and a collision node.

        Returns:
            The parsed file.
        """
        return read_nif_bytes(
            nif(
                ("NiNode", av_body("root", children=(1, 3))),
                ("NiTriShape", av_body("shape", tail=struct.pack("<ii", 2, -1))),
                ("NiTriShapeData", shape_data(SQUARE, ONE_TRIANGLE)),
                ("RootCollisionNode", av_body("collide")),
            ),
            geometry=True,
        )

    def test_the_hierarchy_is_nested_under_its_root(self) -> None:
        """A flat list would lose the thing the pane exists to show."""
        roots = block_tree(self._parsed())
        assert len(roots) == 1
        assert roots[0].type_name == "NiNode"
        assert [child.type_name for child in roots[0].children] == [
            "NiTriShape",
            "RootCollisionNode",
        ]

    def test_blocks_that_never_draw_are_listed(self) -> None:
        """Collision is invisible in a render and decides whether you fall through it.

        This is the whole argument for the pane: the 3D view structurally
        cannot show it.
        """
        roots = block_tree(self._parsed())
        collision = [c for c in roots[0].children if c.type_name == "RootCollisionNode"]
        assert collision and collision[0].note == "collision"

    def test_data_blocks_carry_their_counts(self) -> None:
        """So a shape's weight is visible without selecting it."""
        roots = block_tree(self._parsed())
        shape = roots[0].children[0]
        data = [c for c in shape.children if c.type_name == "NiTriShapeData"]
        assert data and "3 verts" in data[0].note and "1 tris" in data[0].note

    def test_orphans_are_still_shown(self) -> None:
        """A block nothing references is exactly what a reader wants to know about."""
        parsed = read_nif_bytes(
            nif(
                ("NiNode", av_body("root")),
                ("NiZBufferProperty", text("orphan") + struct.pack("<iiH", -1, -1, 0)),
            ),
            geometry=True,
        )
        assert [n.type_name for n in block_tree(parsed)] == ["NiNode", "NiZBufferProperty"]

    def test_a_cycle_does_not_hang(self) -> None:
        """Nothing in the format forbids a child link pointing back up."""
        parsed = read_nif_bytes(
            nif(
                ("NiNode", av_body("a", children=(1,))),
                ("NiNode", av_body("b", children=(0,))),
            ),
            geometry=True,
        )
        roots = block_tree(parsed)
        assert roots, "a cycle should still produce a tree"
