"""Flat land of the patch's own: the record, and making it in an editor session."""

from __future__ import annotations

import base64
import struct

import pytest

from wraithguard.esp.json import record_from_json
from wraithguard.patch.newland import LandError, flat_land


def test_flat_land_is_one_height_everywhere():
    record = flat_land((40, -12), height=250.0, texture_index=3)
    assert record["type"] == "Landscape"
    assert record["grid"] == [40, -12]
    assert record["landscape_flags"] == "USES_VERTEX_HEIGHTS_AND_NORMALS | USES_TEXTURES"
    land = record_from_json(record)
    assert land.grid == (40, -12)
    assert land.vertex_heights_offset == 250.0
    assert land.vertex_heights == bytes(65 * 65)  # no step from any vertex to the next
    assert land.vertex_normals == bytes((0, 0, 127)) * (65 * 65)  # straight up
    assert struct.unpack("<256H", land.texture_indices) == (3,) * 256
    assert land.world_map_data == struct.pack("b", 2) * 81  # 250 / 128, rounded


def test_flat_land_at_the_sea_has_the_default_ground():
    land = record_from_json(flat_land((0, 0)))
    assert land.vertex_heights_offset == 0.0
    assert land.texture_indices == bytes(512)


def test_flat_land_below_the_sea_reads_as_water_on_the_world_map():
    land = record_from_json(flat_land((0, 0), height=-1000.0))
    assert land.world_map_data == struct.pack("b", -8) * 81


@pytest.mark.parametrize("height", [float("nan"), float("inf"), 1e9, -1e9, "high", None])
def test_flat_land_refuses_a_height_out_of_range(height):
    with pytest.raises(LandError):
        flat_land((0, 0), height=height)


@pytest.mark.parametrize("index", [-1, 0x10000, 1.5, True])
def test_flat_land_refuses_a_texture_index_a_cell_cannot_hold(index):
    with pytest.raises(LandError):
        flat_land((0, 0), texture_index=index)


def test_the_record_is_what_the_writer_takes():
    """Every packed grid is base64 of what the record holds (zstd'd when a backend is there)."""
    record = flat_land((1, 2))
    for name in ("vertex_normals", "world_map_data", "vertex_colors", "texture_indices"):
        assert base64.b64decode(record[name]["data"])
    assert record["vertex_heights"]["offset"] == 0.0
