"""The mesh summary Resource Conflicts reports from (wraithguard.nif.report).

The meshes are built byte by byte here and read by greatness7's ``tes3::nif``
(``wraithguard_native.nif_summary``): shapes and their counts, the textures named
(normalised), collision and animation, and how two summaries compare.
"""

from __future__ import annotations

import struct

import pytest

from wraithguard.nif import NifParseError, Structure, compare, summarise
from wraithguard.nif.report import normalise_texture, texture_key

HEADER = b"NetImmerse File Format, Version 4.0.0.2\n"


HEADER = b"NetImmerse File Format, Version 4.0.0.2\n"


def text(value: str) -> bytes:
    """Encode a length-prefixed, unterminated string as a NIF stores one.

    Args:
        value: The text.

    Returns:
        The encoded bytes.
    """
    raw = value.encode("cp1252")
    return struct.pack("<I", len(raw)) + raw


def nif(*blocks: tuple[str, bytes], version: int = 0x04000002) -> bytes:
    """Assemble a whole file from block bodies.

    Args:
        blocks: ``(type name, body bytes)`` pairs in file order.
        version: The version word to write.

    Returns:
        The file bytes.
    """
    out = [HEADER, struct.pack("<I", version), struct.pack("<I", len(blocks))]
    for type_name, body in blocks:
        out.append(text(type_name))
        out.append(body)
    out.append(struct.pack("<Ii", 1, 0))  # the root list: block 0
    return b"".join(out)


def av_object(name: str, *, properties: int = 0, children: int = 0) -> bytes:
    """Build the shared scene-object preamble plus node tails.

    Args:
        name: The object's name.
        properties: How many property links to write.
        children: How many child links to write, ``-1`` if negative.

    Returns:
        The body bytes for a ``NiNode``.
    """
    body = [
        text(name),
        struct.pack("<i", -1),  # extra data
        struct.pack("<i", -1),  # controller
        struct.pack("<H", 0),  # flags
        b"\0" * 12,  # translation
        b"\0" * 36,  # rotation
        struct.pack("<f", 1.0),  # scale
        b"\0" * 12,  # velocity
        struct.pack("<I", properties),
        b"".join(struct.pack("<i", 9) for _ in range(properties)),
        struct.pack("<I", 0),  # has bounding box
    ]
    body.append(struct.pack("<I", children))
    body.append(b"".join(struct.pack("<i", 1) for _ in range(children)))
    body.append(struct.pack("<I", 0))  # effects
    return b"".join(body)


def tri_shape(name: str, *, data: int = 1) -> bytes:
    """Build a ``NiTriShape`` body.

    Args:
        name: The shape's name.
        data: The block index of its geometry data.

    Returns:
        The body bytes.
    """
    head = av_object(name)
    # av_object appended the node tail; rebuild without it.
    head = head[: -(4 + 4)]
    return head + struct.pack("<ii", data, -1)


def tri_shape_data(vertices: int, triangles: int) -> bytes:
    """Build a ``NiTriShapeData`` body with no optional arrays.

    Args:
        vertices: The vertex count to declare.
        triangles: The triangle count to declare.

    Returns:
        The body bytes.
    """
    return b"".join(
        [
            struct.pack("<H", vertices),
            struct.pack("<I", 1),  # has vertices
            b"\0" * (vertices * 12),
            struct.pack("<I", 0),  # has normals
            b"\0" * 12,  # center
            struct.pack("<f", 1.0),  # radius
            struct.pack("<I", 0),  # has vertex colors
            struct.pack("<H", 0),  # uv sets
            struct.pack("<I", 0),  # has uv
            struct.pack("<H", triangles),
            struct.pack("<I", triangles * 3),
            b"\0" * (triangles * 6),
            struct.pack("<H", 0),  # match groups
        ]
    )


class TestTextureReferences:
    """ "Which texture does this mesh ask for" is a question about the file."""

    def test_an_external_source_texture_yields_its_filename(self) -> None:
        """The answer a resource-conflict report actually needs."""
        got = TestStructureReport().structure(textures=("textures\\tx_rock_01.dds",))
        assert got.textures == ["textures/tx_rock_01.dds"]

    def test_an_internal_texture_reports_no_filename(self) -> None:
        """Embedded pixels reference nothing on disk, so nothing is claimed."""
        body = (
            text("")
            + struct.pack("<ii", -1, -1)
            + struct.pack("<B", 0)  # not external
            + struct.pack("<B", 1)  # internal: a pixel data link (none here)
            + struct.pack("<i", -1)
            + struct.pack("<III", 0, 0, 0)
            + struct.pack("<B", 1)
        )
        assert summarise(nif(("NiSourceTexture", body))).textures == []


class TestStructureReport:
    """The questions a resource conflict actually raises."""

    @staticmethod
    def mesh(
        *,
        shapes: tuple[tuple[str, int, int], ...] = (),
        textures: tuple[str, ...] = (),
        collision: bool = False,
        animation: bool = False,
    ) -> bytes:
        """Build a whole mesh with the features under test.

        Args:
            shapes: ``(name, vertices, triangles)`` per shape.
            textures: External texture paths to reference.
            collision: Whether to include a collision node.
            animation: Whether to include a keyframe controller.

        Returns:
            The file bytes.
        """
        blocks: list[tuple[str, bytes]] = [("NiNode", av_object("Root"))]
        for name, verts, tris in shapes:
            data_index = len(blocks) + 1
            blocks.append(("NiTriShape", tri_shape(name, data=data_index)))
            blocks.append(("NiTriShapeData", tri_shape_data(verts, tris)))
        blocks.extend(
            (
                "NiSourceTexture",
                text("")
                + struct.pack("<ii", -1, -1)
                + struct.pack("<B", 1)
                + text(path)
                + struct.pack("<III", 0, 0, 0)
                + struct.pack("<B", 1),
            )
            for path in textures
        )
        if collision:
            blocks.append(("RootCollisionNode", av_object("Collision")))
        if animation:
            blocks.append(
                (
                    "NiKeyframeController",
                    struct.pack("<i", -1)
                    + struct.pack("<H", 8)
                    + struct.pack("<ffff", 1.0, 0.0, 0.0, 1.0)
                    + struct.pack("<ii", 0, -1),
                )
            )
        return nif(*blocks)

    def structure(self, **kwargs: object) -> Structure:
        """Build a mesh and summarise it.

        Args:
            kwargs: Passed to :meth:`mesh`.

        Returns:
            The structure summary.
        """
        return summarise(self.mesh(**kwargs))  # type: ignore[arg-type]

    def test_shapes_carry_their_counts(self) -> None:
        """ "Is the winner a tenth the polys" is the first question asked."""
        got = self.structure(shapes=(("Body", 100, 60), ("Head", 40, 20)))

        assert [(s.name, s.vertices, s.triangles) for s in got.shapes] == [
            ("Body", 100, 60),
            ("Head", 40, 20),
        ]
        assert got.total_triangles == 80

    def test_texture_references_are_normalised(self) -> None:
        """Two spellings of one path are one texture, not two.

        Morrowind paths are case-insensitive and written with either slash, and
        mods are inconsistent about both.
        """
        got = self.structure(textures=("Textures\\TX_Rock_01.DDS", "textures/tx_rock_01.dds"))

        assert got.textures == ["textures/tx_rock_01.dds"]

    def test_collision_and_animation_are_detected(self) -> None:
        """Both are presence questions rather than flags on anything."""
        assert self.structure(collision=True).has_collision
        assert not self.structure().has_collision
        assert self.structure(animation=True).has_animation
        assert not self.structure().has_animation

    def test_an_unreadable_file_is_an_error_not_an_empty_mesh(self) -> None:
        """An absence proves nothing when the file was not read.

        The crate reads a file whole or not at all, so a file it cannot read is a
        NifParseError with the reason - never a summary claiming "no collision".
        """
        with pytest.raises(NifParseError):
            summarise(nif(("NiNode", av_object("Root")), (UNKNOWN_TYPE, b"")))

    def test_a_shape_whose_data_is_missing_reports_zero_not_a_guess(self) -> None:
        """An unknown size is not an empty one."""
        got = summarise(nif(("NiTriShape", tri_shape("Orphan", data=-1))))

        assert got.shapes[0].vertices == 0
        assert got.shapes[0].name == "Orphan"


class TestComparingTwoMeshes:
    """What changes when the winner replaces the loser."""

    @staticmethod
    def built(**kwargs: object) -> Structure:
        """Summarise a mesh built from the shared fixture.

        Args:
            kwargs: Passed to :meth:`TestStructureReport.mesh`.

        Returns:
            The structure summary.
        """
        return TestStructureReport().structure(**kwargs)

    def test_a_simplified_winner_shows_as_a_ratio(self) -> None:
        """The number that says "this is a downgrade" without adjectives."""
        loser = self.built(shapes=(("Body", 100, 100),))
        winner = self.built(shapes=(("Body", 10, 10),))

        assert compare(loser, winner).triangle_ratio == pytest.approx(0.1)

    def test_a_ratio_against_nothing_is_not_invented(self) -> None:
        """Dividing by zero triangles would report an infinite downgrade."""
        loser = self.built()
        winner = self.built(shapes=(("Body", 10, 10),))

        assert compare(loser, winner).triangle_ratio is None

    def test_lost_collision_is_reported(self) -> None:
        """Found in game by falling through the world, otherwise."""
        difference = compare(self.built(collision=True), self.built())

        assert difference.lost_collision

    def test_gained_collision_is_not_reported_as_a_loss(self) -> None:
        """Only losses are named; a report of every difference is unread."""
        difference = compare(self.built(), self.built(collision=True))

        assert not difference.lost_collision

    def test_lost_animation_is_reported(self) -> None:
        """A door whose winning mesh has no controllers stops moving."""
        assert compare(self.built(animation=True), self.built()).lost_animation

    def test_texture_references_are_split_both_ways(self) -> None:
        """A mesh asking for a texture nobody ships is the subtle breakage."""
        loser = self.built(textures=("textures/old.dds",))
        winner = self.built(textures=("textures/new.dds",))

        difference = compare(loser, winner)

        assert difference.added_textures == ["textures/new.dds"]
        assert difference.dropped_textures == ["textures/old.dds"]

    def test_a_partial_read_makes_the_comparison_unreliable(self) -> None:
        """Every absence in the result is unproven, so the flag travels with it."""
        partial = Structure(blocks_read=1, blocks_declared=3, stopped_reason="stopped at block 1")

        assert compare(partial, self.built()).unreliable
        assert compare(self.built(), partial).unreliable
        assert not compare(self.built(), self.built()).unreliable


UNKNOWN_TYPE = "NiDefinitelyNotARealBlock"


def property_body(name: str = "") -> bytes:
    """A minimal NiProperty body: the named-object preamble plus flags.

    Args:
        name: The block's name.

    Returns:
        The body bytes.
    """
    return text(name) + struct.pack("<iiH", -1, -1, 0)


def time_controller(data_link: int = -1) -> bytes:
    """The shared NiTimeController preamble plus a data link.

    Args:
        data_link: The block index the controller drives.

    Returns:
        The body bytes.
    """
    return struct.pack("<ihffffii", -1, 0, 1.0, 0.0, 0.0, 1.0, -1, data_link)


def texture_slot(link: int, *, bump: bool = False) -> bytes:
    """One present texture slot: the flag, the link and the descriptor.

    Args:
        link: The ``NiSourceTexture`` block index.
        bump: Whether this is the bump slot, which carries a luma scale and
            offset plus a 2x2 matrix on top of the usual descriptor.

    Returns:
        The 26 bytes a present slot occupies, or 50 for the bump slot.
    """
    slot = struct.pack("<Ii", 1, link) + struct.pack("<IIIhhh", 0, 0, 0, 0, 0, 0)
    return slot + struct.pack("<6f", *([0.0] * 6)) if bump else slot


def texture_slots(count: int, link: int) -> bytes:
    """A full run of present slots, with the bump slot written correctly.

    Args:
        count: How many slots to write.
        link: The block index every slot points at.

    Returns:
        The concatenated slots.
    """
    return b"".join(texture_slot(link, bump=index == 5) for index in range(count))


def skin_bone(weighted: int) -> bytes:
    """One bone's entry in a ``NiSkinData``.

    Args:
        weighted: How many vertices this bone influences.

    Returns:
        The transform and bounding sphere, the count, and the weight pairs.
    """
    fixed = struct.pack("<9f", *([0.0] * 9)) + struct.pack("<4f", 0.0, 0.0, 0.0, 1.0)
    fixed += struct.pack("<4f", 0.0, 0.0, 0.0, 1.0)
    return fixed + struct.pack("<H", weighted) + struct.pack("<Hf", 0, 1.0) * weighted


class TestTextureIdentityIgnoresTheExtension:
    """A reference names a file by path and stem; the engine picks the format.

    Base-game meshes routinely say ``.bmp`` or ``.tga`` for files that only
    ever shipped as ``.dds``, and Morrowind loads them regardless. Comparing
    references verbatim therefore invents differences -- and it did: two
    versions of one mesh naming the same texture with different extensions
    were reported as one adding a texture and the other dropping it, on the
    line a user is most likely to act on.
    """

    def test_the_same_texture_named_differently_is_not_a_difference(self) -> None:
        """The exact case seen in the wild: darkbrotherhood_head.bmp vs .dds."""
        loser = Structure(textures=[normalise_texture("darkbrotherhood_head.bmp")])
        winner = Structure(textures=[normalise_texture("darkbrotherhood_head.dds")])
        difference = compare(loser, winner)
        assert difference.added_textures == []
        assert difference.dropped_textures == []

    def test_a_redundant_textures_prefix_is_not_a_difference(self) -> None:
        """Some exporters write it, some do not; it names the same file."""
        difference = compare(
            Structure(textures=[normalise_texture("tx_rock.tga")]),
            Structure(textures=[normalise_texture("textures/tx_rock.dds")]),
        )
        assert difference.added_textures == []

    def test_genuinely_different_textures_are_still_reported(self) -> None:
        """A negative control.

        Without this, the fix could be "never report a texture difference",
        which would pass every assertion above and destroy the finding.
        """
        difference = compare(
            Structure(textures=[normalise_texture("tx_a.dds")]),
            Structure(textures=[normalise_texture("tx_b.dds")]),
        )
        assert difference.added_textures == ["tx_b.dds"]
        assert difference.dropped_textures == ["tx_a.dds"]

    def test_the_report_still_shows_the_reference_as_written(self) -> None:
        """Identity is for comparing; a person wants to see what the mesh says."""
        difference = compare(
            Structure(textures=[normalise_texture("tx_a.tga")]),
            Structure(textures=[normalise_texture("tx_b.BMP")]),
        )
        assert difference.added_textures == ["tx_b.bmp"]

    def test_a_directory_with_a_dot_is_not_mistaken_for_an_extension(self) -> None:
        """``mod.v2/rock`` has no extension, and stripping one would corrupt it."""
        assert texture_key("mod.v2/rock") == "mod.v2/rock"
        assert texture_key("mod.v2/rock.dds") == "mod.v2/rock"


def _node_with_property_count(count: int) -> bytes:
    """A ``NiNode`` body whose property-link count is ``count``, links omitted.

    Used to drive the implausible-count guard: a real body would follow the
    count with that many links, but the guard fires on the count itself.

    Args:
        count: The value to write where the property-link count goes.

    Returns:
        The node body bytes, truncated right after the count.
    """
    return (
        text("n")
        + struct.pack("<ii", -1, -1)  # extra data, controller
        + struct.pack("<H", 0)  # flags
        + b"\0" * 12  # translation
        + b"\0" * 36  # rotation
        + struct.pack("<f", 1.0)  # scale
        + b"\0" * 12  # velocity
        + struct.pack("<I", count)  # property-link count
    )
