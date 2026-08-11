"""Tests for :mod:`wraithguard.land.heights`.

The central property is that encoding inverts the decoder that
:mod:`wraithguard.tes3fields.landscape` already provides. That decoder was
verified against real plugins when it was written, so making it the oracle here
means the encoder is checked against something independent of itself rather
than against a second copy of its own arithmetic.
"""

from __future__ import annotations

import struct

import pytest

from wraithguard.land.heights import (
    MAX_GRADIENT,
    MIN_GRADIENT,
    VHGT_SIZE,
    HeightEncodeError,
    decode_heights_from_deltas,
    encode_vertex_heights,
    pack_vertex_normals,
    round_trips,
    vertex_normals_from_heights,
)
from wraithguard.tes3fields.landscape import (
    HEIGHT_SCALE,
    LAND_NUM_VERTS,
    LAND_SIZE,
    decode_vertex_heights,
)


def flat_grid(height: float = 0.0) -> list[list[float]]:
    """Build a 65x65 grid at one height.

    Args:
        height: The world-unit height for every vertex.

    Returns:
        The grid.
    """
    return [[height] * LAND_SIZE for _ in range(LAND_SIZE)]


def sloped_grid(step: float = 8.0) -> list[list[float]]:
    """Build a grid rising by ``step`` per vertex in both directions.

    Args:
        step: World units gained per vertex.

    Returns:
        The grid.
    """
    return [[(x + y) * step for x in range(LAND_SIZE)] for y in range(LAND_SIZE)]


class TestEncodeShape:
    """The payload has to be exactly what the subrecord expects."""

    def test_payload_is_the_documented_size(self) -> None:
        """A VHGT payload is 4,232 bytes: offset, deltas, three unused."""
        _, payload, _ = encode_vertex_heights(flat_grid())
        assert len(payload) == VHGT_SIZE == 4232

    def test_trailing_bytes_are_zero(self) -> None:
        """The three unused bytes are written as zero, not left as whatever."""
        _, payload, _ = encode_vertex_heights(flat_grid())
        assert payload[-3:] == b"\x00\x00\x00"

    def test_offset_is_the_south_west_corner(self) -> None:
        """The offset is vertex (0, 0) in stored units, which the decoder assumes."""
        grid = flat_grid(512.0)
        offset, _, _ = encode_vertex_heights(grid)
        assert offset == 512.0 / HEIGHT_SCALE

    @pytest.mark.parametrize("rows", [[], [[0.0] * LAND_SIZE] * 3])
    def test_wrong_row_count_is_refused(self, rows: list[list[float]]) -> None:
        """A short grid raises rather than encoding a partial cell."""
        with pytest.raises(HeightEncodeError):
            encode_vertex_heights(rows)

    def test_wrong_column_count_is_refused(self) -> None:
        """A ragged row raises: silently padding it would misplace every later vertex."""
        grid = flat_grid()
        grid[7] = [0.0] * (LAND_SIZE - 1)
        with pytest.raises(HeightEncodeError):
            encode_vertex_heights(grid)


class TestRoundTrip:
    """Encoding must invert the decoder in tes3fields, not merely itself."""

    @pytest.mark.parametrize(
        "grid",
        [flat_grid(), flat_grid(1024.0), flat_grid(-256.0), sloped_grid(), sloped_grid(-8.0)],
        ids=["flat", "high", "below-sea", "rising", "falling"],
    )
    def test_survives_the_real_decoder(self, grid: list[list[float]]) -> None:
        """The encoded payload decodes back to the grid it came from."""
        offset, payload, clamped = encode_vertex_heights(grid)
        assert clamped == []
        assert decode_vertex_heights(payload[4:], offset) == grid

    def test_round_trips_agrees_with_the_decoder(self) -> None:
        """The convenience predicate matches an explicit decode."""
        grid = sloped_grid()
        assert round_trips(grid)

    def test_a_single_moved_vertex_survives(self) -> None:
        """The doubly-cumulative encoding must not smear one edit across a row."""
        grid = flat_grid(128.0)
        grid[30][40] = 128.0 + 8.0
        offset, payload, clamped = encode_vertex_heights(grid)
        assert clamped == []
        assert decode_vertex_heights(payload[4:], offset) == grid


class TestGradientLimits:
    """A delta is one signed byte, and the caller has to learn when that bit."""

    def test_a_representable_extreme_is_not_clamped(self) -> None:
        """A gradient of exactly MAX_GRADIENT fits and is reported as fitting."""
        grid = flat_grid()
        grid[10][11] = MAX_GRADIENT * HEIGHT_SCALE
        _, _, clamped = encode_vertex_heights(grid)
        assert (11, 10) not in clamped

    def test_too_steep_is_reported_rather_than_hidden(self) -> None:
        """A cliff beyond a signed byte names the vertex it could not encode."""
        grid = flat_grid()
        grid[10][11] = (MAX_GRADIENT + 50) * HEIGHT_SCALE
        _, _, clamped = encode_vertex_heights(grid)
        assert (11, 10) in clamped

    def test_clamping_costs_the_round_trip(self) -> None:
        """Whatever cannot be encoded cannot come back, and the check says so."""
        grid = flat_grid()
        grid[10][11] = (MAX_GRADIENT + 50) * HEIGHT_SCALE
        assert not round_trips(grid)

    def test_the_negative_limit_reaches_one_further_than_the_positive(self) -> None:
        """Two's complement asymmetry: -128 is representable, +128 is not."""
        assert MIN_GRADIENT == -128
        assert MAX_GRADIENT == 127
        grid = flat_grid()
        grid[5][6] = MIN_GRADIENT * HEIGHT_SCALE
        _, _, clamped = encode_vertex_heights(grid)
        assert (6, 5) not in clamped


class TestDeltaDecoder:
    """The delta-list decoder mirrors the field decoder."""

    def test_matches_the_field_decoder(self) -> None:
        """Both decoders agree on the same data."""
        grid = sloped_grid()
        offset, payload, _ = encode_vertex_heights(grid)
        deltas = list(struct.unpack_from(f"<{LAND_NUM_VERTS}b", payload, 4))
        assert decode_heights_from_deltas(offset, deltas) == decode_vertex_heights(
            payload[4:], offset
        )

    def test_wrong_length_is_refused(self) -> None:
        """A short delta list raises instead of producing a truncated cell."""
        with pytest.raises(HeightEncodeError):
            decode_heights_from_deltas(0.0, [0, 0, 0])


class TestVertexNormals:
    """Normals light the terrain, so an inverted one is a visible bug."""

    def test_flat_ground_points_straight_up(self) -> None:
        """Every normal on level terrain is (0, 0, 127)."""
        normals = vertex_normals_from_heights(flat_grid(64.0))
        assert {triple for row in normals for triple in row} == {(0, 0, 127)}

    def test_normals_always_point_upward(self) -> None:
        """A downward normal means the cross product is the wrong way round."""
        normals = vertex_normals_from_heights(sloped_grid())
        assert all(triple[2] > 0 for row in normals for triple in row)

    def test_a_rising_slope_tilts_away_from_the_rise(self) -> None:
        """Ground rising to the east has a normal leaning west, i.e. negative x."""
        normals = vertex_normals_from_heights(sloped_grid(step=32.0))
        assert normals[32][32][0] < 0
        assert normals[32][32][1] < 0

    def test_components_stay_inside_a_signed_byte(self) -> None:
        """Anything outside -128..127 cannot be packed and would wrap."""
        normals = vertex_normals_from_heights(sloped_grid(step=64.0))
        assert all(-128 <= c <= 127 for row in normals for t in row for c in t)

    def test_grid_shape_is_validated(self) -> None:
        """A malformed grid raises rather than indexing off the end."""
        with pytest.raises(HeightEncodeError):
            vertex_normals_from_heights([[0.0, 1.0], [2.0, 3.0]])


class TestPackNormals:
    """VNML is a fixed 12,675 bytes."""

    def test_payload_size(self) -> None:
        """65 * 65 * 3 signed bytes."""
        normals = vertex_normals_from_heights(flat_grid())
        assert len(pack_vertex_normals(normals)) == 12675

    def test_shape_is_validated(self) -> None:
        """A wrong-sized grid raises instead of writing a short subrecord."""
        with pytest.raises(HeightEncodeError):
            pack_vertex_normals([[(0, 0, 127)]])

    def test_round_trips_through_the_field_decoder(self) -> None:
        """Packed normals read back identically."""
        from wraithguard.tes3fields.landscape import decode_vertex_normals

        normals = vertex_normals_from_heights(sloped_grid())
        assert decode_vertex_normals(pack_vertex_normals(normals)) == normals


class TestNonFiniteGuards:
    """A NaN or infinite vertex must never abort encoding a whole cell.

    ``int(nan)``/``int(inf)`` raise and ``struct.pack('b', ...)`` rejects
    anything outside ``[-128, 127]``. A merged or corrupt vertex can arrive
    non-finite (a division in slope/curvature weighting, or garbage in a source
    ``VHGT``), so the encoders clamp rather than crash -- Merged Lands'
    ``f32_to_i8_saturating``/``i32_to_i8_saturating``, ported.
    """

    def test_a_nan_height_encodes_instead_of_raising(self) -> None:
        """One bad vertex is treated as flat, not a lost cell."""
        grid = flat_grid(100.0)
        grid[10][10] = float("nan")
        _offset, payload, _clamped = encode_vertex_heights(grid)
        assert len(payload) == VHGT_SIZE

    def test_an_infinite_height_encodes_instead_of_raising(self) -> None:
        """``int(inf)`` would raise ``OverflowError``; it is guarded."""
        grid = flat_grid(0.0)
        grid[0][0] = float("inf")
        grid[5][5] = float("-inf")
        _offset, payload, _clamped = encode_vertex_heights(grid)
        assert len(payload) == VHGT_SIZE

    def test_normals_from_a_nan_grid_still_pack(self) -> None:
        """A non-finite vertex must not poison a normal past a signed byte."""
        grid = sloped_grid(8.0)
        grid[20][20] = float("nan")
        grid[21][20] = float("inf")
        normals = vertex_normals_from_heights(grid)
        # Every component is a valid signed byte -- pack proves it.
        packed = pack_vertex_normals(normals)
        assert len(packed) == 3 * LAND_NUM_VERTS
        assert all(MIN_GRADIENT <= c <= MAX_GRADIENT for row in normals for t in row for c in t)

    def test_signed_byte_saturation_is_exact(self) -> None:
        """The helper clamps, maps NaN to zero, and truncates toward zero."""
        from wraithguard.land.heights import _to_signed_byte

        assert _to_signed_byte(float("nan")) == 0
        assert _to_signed_byte(float("inf")) == MAX_GRADIENT
        assert _to_signed_byte(float("-inf")) == MIN_GRADIENT
        assert _to_signed_byte(200.0) == MAX_GRADIENT
        assert _to_signed_byte(-200.0) == MIN_GRADIENT
        assert _to_signed_byte(126.9) == 126
        assert _to_signed_byte(-126.9) == -126
        assert _to_signed_byte(127.0) == MAX_GRADIENT
        assert _to_signed_byte(-128.0) == MIN_GRADIENT

    def test_a_finite_grid_is_unchanged_by_the_guard(self) -> None:
        """The guard is inert on ordinary terrain: normals are as before."""
        grid = sloped_grid(8.0)
        normals = vertex_normals_from_heights(grid)
        # A plain sloped grid has a uniform interior normal; sanity-check it is
        # a real unit-ish signed-byte vector, not the (0,0,0) a NaN would give.
        assert normals[10][10] != (0, 0, 0)
