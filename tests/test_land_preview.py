"""Previewing a conflict strategy on one cell before writing a sidecar.

:func:`merge_preview` answers "what does each strategy do to this cell" with the
first plugin as the base every later one is measured against. The point is that
the four strategies produce visibly different terrain where two plugins contest a
vertex -- and the same terrain where only one plugin edits it.
"""

from __future__ import annotations

import pytest

from wraithguard.land.merge import ConflictStrategy
from wraithguard.land.preview import PREVIEW_STRATEGIES, merge_preview
from wraithguard.tes3fields.landscape import LAND_SIZE


def flat(value: float = 0.0) -> list[list[float]]:
    """A flat 65x65 height grid."""
    return [[value] * LAND_SIZE for _ in range(LAND_SIZE)]


class TestMergePreview:
    def test_the_strategies_disagree_on_a_contested_vertex(self) -> None:
        """Base + two edits to one vertex: each strategy settles it differently.

        The larger edit is +800, the smaller +40; Overwrite takes the later
        (smaller) one, Ignore the earlier (larger) one, and Resolve/Curvature
        land in between, biased toward the larger.
        """
        base, big, small = flat(), flat(), flat()
        big[10][10] = 800.0  # earlier plugin's edit
        small[10][10] = 40.0  # later plugin's edit

        results = {
            strat: merge_preview([base, big, small], strat)[10][10] for strat in PREVIEW_STRATEGIES
        }

        assert results[ConflictStrategy.OVERWRITE] == 40.0
        assert results[ConflictStrategy.IGNORE] == 800.0
        # The blends sit between the two edits, nearer the larger one.
        for blend in (ConflictStrategy.RESOLVE, ConflictStrategy.CURVATURE):
            assert 40.0 < results[blend] < 800.0

    def test_one_editor_is_the_same_under_every_strategy(self) -> None:
        """Nothing is contested, so strategy choice cannot matter."""
        base, edit = flat(), flat()
        edit[5][5] = 500.0
        seen = {tuple(merge_preview([base, edit], s)[5]) for s in PREVIEW_STRATEGIES}
        assert len(seen) == 1

    def test_an_uncontested_vertex_keeps_its_single_edit(self) -> None:
        """Where only one of two plugins moved a vertex, that edit is taken whole."""
        base, a, b = flat(), flat(), flat()
        a[1][1] = 300.0  # only A moves (1,1)
        b[2][2] = 200.0  # only B moves (2,2)
        merged = merge_preview([base, a, b], ConflictStrategy.OVERWRITE)
        assert merged[1][1] == 300.0
        assert merged[2][2] == 200.0

    def test_no_grids_is_an_error(self) -> None:
        """There is nothing to preview, and returning a blank cell would lie."""
        with pytest.raises(ValueError, match="no terrain"):
            merge_preview([], ConflictStrategy.OVERWRITE)

    def test_a_wrong_sized_grid_is_refused(self) -> None:
        """A grid that is not 65x65 cannot be a cell."""
        with pytest.raises(ValueError, match="65x65"):
            merge_preview([[[0.0, 0.0]]], ConflictStrategy.OVERWRITE)


class TestTerrainViewAcceptsAGrid:
    def test_a_decoded_grid_renders_like_an_encoded_surface(self) -> None:
        """The preview hands build_terrain_3d computed grids, not VHGT bytes."""
        from wraithguard.viz.terrain3d import build_terrain_3d

        grid = [[float(y) for _ in range(LAND_SIZE)] for y in range(LAND_SIZE)]
        html = build_terrain_3d({"Merged: Overwrite": grid}, cell_label="(0, 0)")
        assert html.startswith("<!DOCTYPE html>")
        assert '"surfaces"' in html
