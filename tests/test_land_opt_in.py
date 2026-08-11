"""Opt-in / diagnostic land features: CELL merging, debug colours, conflict images.

These carry the ``--cells``, ``--add-debug-vertex-colors`` and ``--conflicts-dir``
paths, which are off by default and so were the tree's least-exercised land code.
All three are pure -- dicts and grids in, records and PNG bytes out -- so a small
synthetic load order is enough to run them and pin their behaviour.
"""

from __future__ import annotations

import pytest

from wraithguard.land.cells import (
    CELL_TYPE,
    _grid_of,
    _union_flags,
    _without_references,
    cells_for,
    merge_cells,
)
from wraithguard.land.conflict_image import cell_conflict_image, landmass_conflict_image
from wraithguard.land.debug_colors import (
    MAJOR_COLOR,
    MINOR_COLOR,
    MODIFIED_COLOR,
    blank_colors,
    paint_conflicts,
)
from wraithguard.land.diff import RelativeGrid
from wraithguard.land.heights import LAND_SIZE

_PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def _flat(value: int = 0) -> list[int]:
    """A land-sized single-component grid at one value."""
    return [value] * (LAND_SIZE * LAND_SIZE)


def _grid_with(x: int, y: int, value: int) -> RelativeGrid:
    """A grid differing from flat-zero only at ``(x, y)``, by ``value``."""
    absolute = _flat(0)
    absolute[y * LAND_SIZE + x] = value
    return RelativeGrid.from_difference(_flat(0), absolute, side=LAND_SIZE)


def _cell(
    x: int,
    y: int,
    *,
    flags: str = "",
    region: str | None = None,
    water: int | None = None,
    cell_id: str | None = None,
    interior: bool = False,
) -> dict:
    """A decoded exterior (or interior) ``Cell`` record as tes3conv emits one."""
    record: dict = {
        "type": CELL_TYPE,
        "flags": flags,
        "data": {"grid": [x, y], "flags": "IS_INTERIOR" if interior else ""},
        "references": [{"mast_index": 0, "refr_index": 1}],
    }
    if region is not None:
        record["region"] = region
    if water is not None:
        record["water_height"] = water
    if cell_id is not None:
        record["id"] = cell_id
    return record


class TestUnionFlags:
    """Two mods' flags accumulate; neither's bit is dropped."""

    def test_it_unions_sorts_and_dedups(self) -> None:
        """Every named bit either side sets survives, in a stable order."""
        assert _union_flags("B | A", "A | C") == "A | B | C"

    def test_empty_sides_contribute_nothing(self) -> None:
        """A blank string is not a bit named empty-string."""
        assert _union_flags("", "X") == "X"
        assert _union_flags("", "") == ""


class TestGridOf:
    """Only exterior cells have a grid position, and only they apply here."""

    def test_an_exterior_cell_yields_its_coords(self) -> None:
        """Negative coordinates are real -- the world spans all four quadrants."""
        assert _grid_of(_cell(3, -4)) == (3, -4)

    def test_an_interior_cell_is_none(self) -> None:
        """An interior cell has no landscape, so nothing here applies."""
        assert _grid_of(_cell(0, 0, interior=True)) is None

    def test_a_record_without_a_grid_is_none(self) -> None:
        """A malformed or gridless record is skipped, not guessed at."""
        assert _grid_of({"type": CELL_TYPE, "data": {}}) is None
        assert _grid_of({"type": CELL_TYPE}) is None


class TestMergeCells:
    """Folding a load order's exterior CELL records into one per cell."""

    def test_two_editors_union_flags_and_take_the_later_scalar(self) -> None:
        """Flags accumulate; region/water are last-wins; references never carry."""
        master = ("Master.esm", [_cell(1, 1, flags="A", region="Bitter Coast", water=-10)])
        mod = ("Mod.esp", [_cell(1, 1, flags="B", region="Ascadian Isles", water=-50, cell_id="C")])
        rec = merge_cells([master, mod])[(1, 1)]
        assert rec.record["flags"] == "A | B"
        assert rec.record["region"] == "Ascadian Isles"
        assert rec.record["water_height"] == -50
        assert rec.record["id"] == "C"
        assert rec.editors == ["Master.esm", "Mod.esp"]
        assert rec.modified is True
        assert rec.record["references"] == []

    def test_a_single_editor_cell_is_not_marked_modified(self) -> None:
        """One editor changed nothing against itself, so nothing was merged."""
        rec = merge_cells([("Mod.esp", [_cell(2, 2, flags="A")])])[(2, 2)]
        assert rec.editors == ["Mod.esp"]
        assert rec.modified is False

    def test_a_repeated_identical_edit_changes_nothing(self) -> None:
        """A second plugin restating the same values leaves it unmodified."""
        a = ("A.esp", [_cell(1, 1, flags="A", region="X")])
        b = ("B.esp", [_cell(1, 1, flags="A", region="X")])
        rec = merge_cells([a, b])[(1, 1)]
        assert rec.modified is False
        assert rec.editors == ["A.esp", "B.esp"]

    def test_interior_and_non_cell_records_are_skipped(self) -> None:
        """Only exterior CELL records take part."""
        sources = [("Mod.esp", [_cell(0, 0, interior=True), {"type": "Static"}, _cell(5, 6)])]
        assert set(merge_cells(sources)) == {(5, 6)}

    def test_a_skipped_plugin_does_not_contribute(self) -> None:
        """A previous merge is skipped rather than folded into its successor."""
        sources = [("Prev.esp", [_cell(1, 1, flags="X")])]
        assert merge_cells(sources, skip=frozenset({"Prev.esp"})) == {}

    def test_the_data_block_flags_are_unioned_too(self) -> None:
        """A cell's data-block flags accumulate just like its record flags,
        even when the plugins declare no top-level record flags."""
        a = ("A.esp", [{"type": CELL_TYPE, "data": {"grid": [7, 7], "flags": "QUASI_WATER"}}])
        b = ("B.esp", [{"type": CELL_TYPE, "data": {"grid": [7, 7], "flags": "FORCE_HIDE_LAND"}}])
        rec = merge_cells([a, b])[(7, 7)]
        assert rec.record["data"]["flags"] == "FORCE_HIDE_LAND | QUASI_WATER"
        assert rec.modified is True


class TestWithoutReferences:
    """The merged record must own no object in the cell."""

    def test_references_are_emptied_and_data_is_copied(self) -> None:
        """The source record is left untouched, and its data is not shared."""
        original = _cell(1, 1, flags="A")
        copy = _without_references(original)
        assert copy["references"] == []
        assert original["references"] == [{"mast_index": 0, "refr_index": 1}]
        copy["data"]["flags"] = "MUTATED"
        assert original["data"]["flags"] != "MUTATED"

    def test_a_record_with_no_data_block_is_tolerated(self) -> None:
        """References are still emptied when the record carries no data block."""
        copy = _without_references({"type": CELL_TYPE, "references": [{"x": 1}]})
        assert copy["references"] == []
        assert copy.get("data") is None


class TestCellsFor:
    """Only cells whose terrain is also written get a CELL record."""

    def test_it_selects_wanted_cells_in_coordinate_order(self) -> None:
        """The output is sorted, and cells not asked for are left out."""
        merged = merge_cells([("Mod.esp", [_cell(2, 2), _cell(0, 0), _cell(1, 1)])])
        got = cells_for(merged, {(0, 0), (2, 2)})
        grids = [tuple(r["data"]["grid"]) for r in got]
        assert grids == [(0, 0), (2, 2)]

    def test_a_wanted_cell_with_no_record_is_skipped(self) -> None:
        """Asking for a cell no plugin edited yields nothing, not a KeyError."""
        merged = merge_cells([("Mod.esp", [_cell(0, 0)])])
        assert cells_for(merged, {(9, 9)}) == []


class TestPaintConflicts:
    """Severity painted into a flat vertex-colour grid, with red held highest."""

    def test_blank_colors_is_land_sized_zeroes(self) -> None:
        """The starting grid is 65x65x3 of nothing painted."""
        colors = blank_colors()
        assert len(colors) == LAND_SIZE * LAND_SIZE * 3
        assert set(colors) == {0}

    def test_a_wrong_length_grid_is_rejected(self) -> None:
        """A mis-sized buffer is a caller error, reported not indexed past."""
        with pytest.raises(ValueError, match="colour components"):
            paint_conflicts([0, 0, 0], None, _grid_with(1, 1, 5))

    def test_no_plugin_difference_paints_nothing(self) -> None:
        """A plugin that changed nothing contributes no colour."""
        colors = blank_colors()
        assert paint_conflicts(colors, None, None) == 0
        assert colors == blank_colors()

    def test_the_first_fold_marks_modified_where_the_plugin_moved(self) -> None:
        """With no accumulated merge, an edited vertex is simply 'modified'."""
        colors = blank_colors()
        assert paint_conflicts(colors, None, _grid_with(4, 5, 20)) == 1
        base = (5 * LAND_SIZE + 4) * 3
        assert tuple(colors[base : base + 3]) == MODIFIED_COLOR

    def test_a_far_compromise_is_a_major_conflict(self) -> None:
        """2000 against 3 lands far from the smaller edit: red."""
        colors = blank_colors()
        paint_conflicts(colors, _grid_with(3, 3, 2000), _grid_with(3, 3, 3))
        base = (3 * LAND_SIZE + 3) * 3
        assert tuple(colors[base : base + 3]) == MAJOR_COLOR

    def test_a_close_compromise_is_a_minor_conflict(self) -> None:
        """10 against 8 lands close to both: yellow."""
        colors = blank_colors()
        paint_conflicts(colors, _grid_with(3, 3, 10), _grid_with(3, 3, 8))
        base = (3 * LAND_SIZE + 3) * 3
        assert tuple(colors[base : base + 3]) == MINOR_COLOR

    def test_red_is_never_demoted_to_yellow(self) -> None:
        """A later minor conflict must not conceal an earlier major one."""
        colors = blank_colors()
        base = (3 * LAND_SIZE + 3) * 3
        colors[base], colors[base + 1], colors[base + 2] = MAJOR_COLOR
        painted = paint_conflicts(colors, _grid_with(3, 3, 10), _grid_with(3, 3, 8))
        assert tuple(colors[base : base + 3]) == MAJOR_COLOR
        assert painted == 0

    def test_only_one_side_moving_a_vertex_takes_the_single_side_branches(self) -> None:
        """A vertex only the plugin moved is 'modified'; one only the merge
        moved is left untouched -- the two one-sided returns of the classifier."""
        colors = blank_colors()
        left = _grid_with(1, 1, 5)  # the merge moved (1, 1)
        right = _grid_with(2, 2, 9)  # the plugin moved (2, 2)
        paint_conflicts(colors, left, right)
        moved = (2 * LAND_SIZE + 2) * 3
        merge_only = (1 * LAND_SIZE + 1) * 3
        assert tuple(colors[moved : moved + 3]) == MODIFIED_COLOR
        assert tuple(colors[merge_only : merge_only + 3]) == (0, 0, 0)

    def test_repainting_the_same_colour_is_a_noop(self) -> None:
        """A vertex already at the colour a fold would paint is left alone."""
        colors = blank_colors()
        base = (4 * LAND_SIZE + 4) * 3
        colors[base], colors[base + 1], colors[base + 2] = MODIFIED_COLOR
        painted = paint_conflicts(colors, None, _grid_with(4, 4, 7))
        assert painted == 0
        assert tuple(colors[base : base + 3]) == MODIFIED_COLOR

    def test_identical_edits_on_both_sides_are_skipped_and_stay_minor(self) -> None:
        """Both sides moving a vertex the same amount leaves nothing to weigh:
        that component is skipped and the vertex stays a minor conflict."""
        colors = blank_colors()
        paint_conflicts(colors, _grid_with(5, 5, 12), _grid_with(5, 5, 12))
        base = (5 * LAND_SIZE + 5) * 3
        assert tuple(colors[base : base + 3]) == MINOR_COLOR


class TestConflictImages:
    """The per-cell and whole-landmass PNG maps."""

    def test_a_landmass_map_needs_at_least_one_cell(self) -> None:
        """An empty map is a caller error, not a zero-sized image."""
        with pytest.raises(ValueError, match="no cells"):
            landmass_conflict_image({})

    def test_a_landmass_map_is_a_png(self) -> None:
        """One block per cell, across the world-grid extent."""
        png = landmass_conflict_image({(0, 0): "major", (2, 1): "minor", (1, 0): "clean"})
        assert png.startswith(_PNG_MAGIC)

    def test_a_cell_map_with_a_major_conflict_is_a_png(self) -> None:
        """A contested vertex exercises the modified/minor/major ladder."""
        png = cell_conflict_image(_grid_with(3, 3, 2000), _grid_with(3, 3, 3))
        assert png.startswith(_PNG_MAGIC)

    def test_a_cell_map_with_a_minor_conflict_is_a_png(self) -> None:
        """A close compromise takes the minor branch without the major break."""
        png = cell_conflict_image(_grid_with(3, 3, 10), _grid_with(3, 3, 8))
        assert png.startswith(_PNG_MAGIC)

    def test_a_cell_map_where_both_sides_agree_is_a_png(self) -> None:
        """Identical deltas skip the per-component weigh and stay minor."""
        png = cell_conflict_image(_grid_with(3, 3, 12), _grid_with(3, 3, 12))
        assert png.startswith(_PNG_MAGIC)

    def test_a_plugin_only_edit_still_draws(self) -> None:
        """No merged grid at all: the plugin's own edits are 'modified'."""
        png = cell_conflict_image(None, _grid_with(1, 1, 20))
        assert png.startswith(_PNG_MAGIC)

    def test_an_empty_cell_map_is_still_a_png(self) -> None:
        """Nothing touched: a valid all-untouched tile, not a crash."""
        png = cell_conflict_image(None, None)
        assert png.startswith(_PNG_MAGIC)
