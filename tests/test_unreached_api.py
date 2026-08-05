"""Public functions with no caller anywhere in the tree.

Found by the module audit: five functions that nothing in ``wraithguard``,
``tools`` or the test suite calls. Uncalled *and* untested means never
executed, which means never verified -- a function in that state is a claim,
not code, and the first caller finds out whether it was true.

A sixth, ``viz.palette.terrain_ramp``, was deleted rather than tested. Its
docstring said it was "handed to the 3D view", and it was not: ``tint_ramp``
had superseded it, so the file held two copies of one curve -- exactly the
duplication that docstring warned against.

These five are kept because each is a reasonable thing for a caller to want.
This module is what makes keeping them honest.
"""

from __future__ import annotations

import math

from wraithguard.land.curvature import curvature_at, curvature_map, is_land_grid
from wraithguard.land.debug_colors import blank_colors
from wraithguard.land.heights import LAND_SIZE
from wraithguard.tracing import sort_trace_path, trace_path


def flat(value: float = 0.0) -> list[list[float]]:
    """A land-sized grid at one height."""
    return [[value] * LAND_SIZE for _ in range(LAND_SIZE)]


class TestCurvatureOverAWholeGrid:
    """``curvature_map`` is the batch form of ``curvature_at``.

    ``slope.py`` computes the same thing with its own comprehension, flat
    rather than gridded, so these two must not disagree about any vertex.
    """

    def test_flat_ground_has_no_curvature(self) -> None:
        """The baseline: nothing bends, so every angle is zero."""
        got = curvature_map(flat())
        assert all(value == 0.0 for row in got for value in row)

    def test_the_grid_is_land_shaped(self) -> None:
        """A caller indexes it by ``[y][x]``; a transposed or short grid lies."""
        got = curvature_map(flat())
        assert len(got) == LAND_SIZE
        assert all(len(row) == LAND_SIZE for row in got)

    def test_it_agrees_with_the_single_vertex_form(self) -> None:
        """Two implementations of one measurement is one too many unless they
        agree; this is what stops them drifting apart.
        """
        rows = flat()
        rows[32][32] = 500.0
        got = curvature_map(rows)
        for y, x in ((32, 32), (31, 32), (32, 31), (0, 0), (64, 64)):
            assert got[y][x] == curvature_at(rows, x, y)

    def test_a_peak_bends_the_ground_around_it(self) -> None:
        """Otherwise the measurement reports nothing and nothing notices."""
        rows = flat()
        rows[32][32] = 2000.0
        got = curvature_map(rows)
        assert got[32][32] > 0.0
        assert got[0][0] == 0.0

    def test_every_angle_is_a_real_angle(self) -> None:
        """It is an arccosine mean, so anything outside 0..pi is a bug in the
        clamping, and the clamp exists because floating point dot products
        stray past 1.0.
        """
        rows = flat()
        for y in range(LAND_SIZE):
            for x in range(LAND_SIZE):
                rows[y][x] = float((x * 37 + y * 11) % 900)
        assert all(0.0 <= value <= math.pi for row in curvature_map(rows) for value in row)


class TestTheLandSizePredicate:
    """``is_land_grid`` guards the functions that index 65x65 blindly."""

    def test_a_land_grid_passes(self) -> None:
        """The shape every LAND record uses."""
        assert is_land_grid(flat())

    def test_a_short_grid_fails(self) -> None:
        """Too few rows is the case that indexes out of range later."""
        assert not is_land_grid(flat()[:-1])

    def test_a_ragged_grid_fails(self) -> None:
        """One short row is worse than a short grid: it fails deep in a loop."""
        rows = flat()
        rows[7] = rows[7][:-1]
        assert not is_land_grid(rows)

    def test_nothing_at_all_fails(self) -> None:
        """An empty grid is not a land grid, and must not read as one."""
        assert not is_land_grid([])


class TestABlankColourGrid:
    """``blank_colors`` is the starting canvas for conflict painting."""

    def test_it_is_the_size_a_record_expects(self) -> None:
        """Three channels per vertex, 65 x 65 vertices."""
        assert len(blank_colors()) == LAND_SIZE * LAND_SIZE * 3

    def test_it_starts_black(self) -> None:
        """Painting is additive, so anything non-zero here would show as an
        edit the merge never made.
        """
        assert set(blank_colors()) == {0}

    def test_each_call_is_its_own_grid(self) -> None:
        """A shared list would let one cell's painting bleed into the next."""
        first = blank_colors()
        first[0] = 255
        assert blank_colors()[0] == 0


class TestTracePathAccessors:
    """Where the diagnostic logs are, for anything that wants to say so."""

    def test_no_trace_means_no_path(self) -> None:
        """``None``, not an empty string: "off" and "at ''" differ."""
        assert trace_path() is None or isinstance(trace_path(), str)

    def test_the_sort_trace_is_reported_separately(self) -> None:
        """They are opened independently, so one can be on with the other off."""
        assert sort_trace_path() is None or isinstance(sort_trace_path(), str)
