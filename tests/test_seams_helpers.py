"""The isolated helpers behind seam repair: vertex sharing, pinning, normal masking.

The repair passes themselves are driven by the merge pipeline's tests, but these
small pure helpers -- which cells a vertex touches, whether it is pinned by absent
terrain, and preserving hand-authored normals where the height did not move -- are
easy to pin directly and were otherwise only reached incidentally.
"""

from __future__ import annotations

from array import array

from wraithguard.land.seams import (
    _shared_cells,
    is_pinned,
    mask_normals_to_moved_heights,
)


class TestSharedCells:
    """Which exterior cells share a given vertex of one cell."""

    def test_a_corner_vertex_touches_four_cells(self) -> None:
        """The (0, 0) corner is shared by this cell and its three neighbours."""
        assert set(_shared_cells((5, 5), 0, 0)) == {(5, 5), (5, 4), (4, 5), (4, 4)}

    def test_an_interior_vertex_touches_only_its_own_cell(self) -> None:
        """A vertex away from every edge belongs to one cell alone."""
        assert _shared_cells((5, 5), 32, 32) == [(5, 5)]


class TestIsPinned:
    """A vertex is pinned when any sharing cell is not being written."""

    def test_a_corner_shared_with_an_absent_cell_is_pinned(self) -> None:
        """The corner touches neighbours the merge is not writing, so it is fixed."""
        cells = {(5, 5): array("i", [0])}
        assert is_pinned(cells, (5, 5), 0, 0) is True

    def test_an_interior_vertex_is_not_pinned(self) -> None:
        """An interior vertex is shared with nothing absent, so it is free to move."""
        cells = {(5, 5): array("i", [0])}
        assert is_pinned(cells, (5, 5), 32, 32) is False


class TestMaskNormals:
    """Recomputed normals are kept only where the height actually moved."""

    def test_moved_vertices_keep_recomputed_and_others_inherit(self) -> None:
        """A moved vertex uses the recomputed normal; an unmoved one keeps its own."""
        normals = [[(1, 1, 1), (2, 2, 2)]]
        original = [[(9, 9, 9), (8, 8, 8)]]
        result = mask_normals_to_moved_heights(normals, original, {(0, 0)})
        assert result == [[(1, 1, 1), (8, 8, 8)]]

    def test_out_of_bounds_original_keeps_the_recomputed_normal(self) -> None:
        """Where the original grid has no such vertex, the recomputed one stands."""
        normals = [[(1, 1, 1)], [(2, 2, 2)]]
        original = [[(9, 9, 9)]]
        result = mask_normals_to_moved_heights(normals, original, set())
        assert result == [[(9, 9, 9)], [(2, 2, 2)]]
