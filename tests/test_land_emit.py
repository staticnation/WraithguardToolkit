"""Tests for :mod:`wraithguard.land.emit` and texture compaction.

These cover the only code in the toolkit that produces a file the game loads,
so the emphasis is on the failures that would *not* raise: a texture grid
written without its flag, a shared table emitted uncompacted, a header with no
masters.
"""

from __future__ import annotations

import pytest

from wraithguard.land.emit import (
    FLAG_TEXTURES,
    EmitError,
    attach_texture_indices,
    build_header,
    build_landscape_record,
    build_plugin,
    build_texture_records,
    encode_field,
    pack_texture_indices,
)
from wraithguard.land.textures import KnownTextures, compact_textures, vtex_of
from wraithguard.tes3fields.landscape import decode_texture_indices

MASTERS = [("Morrowind.esm", 79837557)]


def heights(value: float = 0.0) -> list[list[float]]:
    """A flat 65x65 height grid."""
    return [[value] * 65 for _ in range(65)]


def textures(value: int = 0) -> list[list[int]]:
    """A uniform 16x16 texture grid."""
    return [[value] * 16 for _ in range(16)]


class TestEncodeField:
    """Payloads must be zstd-compressed, because tes3's reader assumes it."""

    def test_a_payload_encodes_to_base64_text(self) -> None:
        """The field is a string in the JSON, not bytes."""
        assert isinstance(encode_field(b"hello"), str)

    def test_the_frame_is_zstd(self) -> None:
        """tes3 runs decode_all unconditionally; plain base64 is rejected."""
        import base64

        raw = base64.b64decode(encode_field(b"hello" * 100))
        assert raw.startswith(b"\x28\xb5\x2f\xfd")


class TestHeader:
    """The masters list is what makes references resolve."""

    def test_masters_are_required(self) -> None:
        """A plugin with no masters loads and is wrong, so this refuses."""
        with pytest.raises(EmitError, match="must declare its masters"):
            build_header([], num_objects=1)

    def test_a_master_with_no_size_is_refused(self) -> None:
        """The engine matches on name *and* size; a guess would be silent."""
        with pytest.raises(EmitError, match="no usable size"):
            build_header([("Morrowind.esm", 0)], num_objects=1)

    def test_the_record_count_is_carried(self) -> None:
        """num_objects has to match what follows."""
        header = build_header(MASTERS, num_objects=7)
        assert header["num_objects"] == 7
        assert header["file_type"] == "Esp"


class TestLandscapeRecord:
    """One merged cell."""

    def test_an_empty_record_is_refused(self) -> None:
        """A LAND with no layers would replace real terrain with nothing."""
        with pytest.raises(EmitError, match="no layers"):
            build_landscape_record((0, 0))

    def test_heights_declare_heights_and_normals(self) -> None:
        """The format stores the two under one flag."""
        record, _ = build_landscape_record((1, 2), heights=heights())
        assert record["landscape_flags"] == "USES_VERTEX_HEIGHTS_AND_NORMALS"
        assert record["grid"] == [1, 2]

    def test_normals_are_recomputed_from_heights(self) -> None:
        """Terrain lit from stale normals looks like a rendering bug."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        assert "vertex_normals" in record

    def test_every_grid_is_present_even_when_unused(self) -> None:
        """tes3 declares all five as plain fields, not Options, so its reader
        requires each one. Omitting world_map_data is rejected outright with
        "missing field `world_map_data`" -- found by an actual write."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        for field in (
            "vertex_heights",
            "vertex_normals",
            "world_map_data",
            "vertex_colors",
            "texture_indices",
        ):
            assert field in record

    def test_an_unused_grid_is_written_but_not_declared(self) -> None:
        """A zero grid the flags do not mention is inert, not a black cell."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        assert "USES_VERTEX_COLORS" not in record["landscape_flags"]
        assert FLAG_TEXTURES not in record["landscape_flags"]

    def test_no_world_map_flag_is_ever_emitted(self) -> None:
        """LandscapeFlags has three bits and none of them is a world map.

        An invented ``USES_WORLD_MAP_DATA`` is rejected with "unrecognized
        named flag", which is how this was established.
        """
        record, _ = build_landscape_record((0, 0), heights=heights())
        assert "WORLD_MAP" not in record["landscape_flags"]


class TestAttachTextures:
    """The seam between the two passes."""

    def test_the_flag_is_added_with_the_payload(self) -> None:
        """Writing 512 bytes without declaring them is read as no texture data."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        attach_texture_indices(record, textures(3))
        assert FLAG_TEXTURES in record["landscape_flags"]
        assert "texture_indices" in record

    def test_existing_flags_survive(self) -> None:
        """Attaching textures must not drop the heights declaration."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        attach_texture_indices(record, textures(3))
        assert "USES_VERTEX_HEIGHTS_AND_NORMALS" in record["landscape_flags"]

    def test_the_flag_is_not_duplicated(self) -> None:
        """Attaching twice should not produce a doubled flag string."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        attach_texture_indices(record, textures(3))
        attach_texture_indices(record, textures(4))
        assert record["landscape_flags"].count(FLAG_TEXTURES) == 1

    def test_the_grid_round_trips(self) -> None:
        """The 4x4 swizzle has to be undone on write and redone on read."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        grid = [[(y * 16 + x) % 40 for x in range(16)] for y in range(16)]
        attach_texture_indices(record, grid)
        assert decode_texture_indices(record["texture_indices"]["data"]) == grid

    def test_a_wrong_shape_is_refused(self) -> None:
        """A short grid would misplace every index after the truncation."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        with pytest.raises(EmitError):
            attach_texture_indices(record, [[0] * 16])


class TestPackTextures:
    """VTEX is stored as sixteen 4x4 blocks, not row-major."""

    def test_payload_size(self) -> None:
        """16 * 16 uint16."""
        assert len(pack_texture_indices(textures())) == 512

    def test_packing_inverts_decoding(self) -> None:
        """Round-tripping a non-uniform grid is the only real check."""
        grid = [[(x * 7 + y * 13) % 64 for x in range(16)] for y in range(16)]
        assert decode_texture_indices(pack_texture_indices(grid)) == grid


class TestCompaction:
    """Only ship LTEX records the merged terrain actually references."""

    def _table(self, count: int) -> KnownTextures:
        """A shared table of ``count`` textures."""
        known = KnownTextures()
        known.observe(
            "a.esp",
            [
                {"type": "LandscapeTexture", "id": f"T{i}", "index": i, "file_name": f"t{i}.tga"}
                for i in range(count)
            ],
        )
        return known

    def test_unused_textures_are_dropped(self) -> None:
        """The point: a big table and a small merge should emit a small table."""
        known = self._table(50)
        _, kept = compact_textures(known, {0, vtex_of(4), vtex_of(9)})
        assert [t.identifier for t in kept] == ["T4", "T9"]

    def test_survivors_are_renumbered_contiguously(self) -> None:
        """Gaps would leave indices nothing defines."""
        known = self._table(50)
        _, kept = compact_textures(known, {0, vtex_of(4), vtex_of(9)})
        assert [t.index for t in kept] == [0, 1]

    def test_the_mapping_moves_grids_to_the_new_numbering(self) -> None:
        """A grid rewritten by this mapping must index the emitted records."""
        known = self._table(50)
        mapping, kept = compact_textures(known, {0, vtex_of(4), vtex_of(9)})
        assert mapping[vtex_of(4)] == vtex_of(kept[0].index)
        assert mapping[vtex_of(9)] == vtex_of(kept[1].index)

    def test_no_texture_is_always_retained(self) -> None:
        """Unpainted terrain is a real value, and must stay translatable."""
        mapping, _ = compact_textures(self._table(5), {0})
        assert mapping[0] == 0

    def test_an_index_with_no_record_is_left_alone(self) -> None:
        """A missing master, already reported. Do not remap it to something real."""
        mapping, kept = compact_textures(self._table(3), {0, vtex_of(99)})
        assert vtex_of(99) not in mapping
        assert kept == []

    def test_file_names_survive_compaction(self) -> None:
        """The emitted LTEX still has to name a texture file."""
        _, kept = compact_textures(self._table(5), {0, vtex_of(2)})
        assert kept[0].file_name == "t2.tga"


class TestTextureRecords:
    """Emitting the compacted table."""

    def test_one_record_per_texture(self) -> None:
        """In index order, which is what the grids assume."""
        known = KnownTextures()
        known.observe("a.esp", [{"type": "LandscapeTexture", "id": "A", "index": 0}])
        _, kept = compact_textures(known, {0, vtex_of(0)})
        records = build_texture_records(kept)
        assert [r["type"] for r in records] == ["LandscapeTexture"]
        assert records[0]["id"] == "A"

    def test_an_empty_table_emits_nothing(self) -> None:
        """A merge that paints with nothing needs no LTEX records."""
        assert build_texture_records([]) == []


class TestPlugin:
    """Assembling the document tes3conv converts."""

    def test_the_header_leads_and_counts_the_rest(self) -> None:
        """num_objects must match, or the plugin is malformed."""
        record, _ = build_landscape_record((0, 0), heights=heights())
        document = build_plugin([record], MASTERS)
        assert document[0]["type"] == "Header"
        assert document[0]["num_objects"] == 1
        assert len(document) == 2
