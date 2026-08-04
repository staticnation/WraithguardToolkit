"""Regression tests for two faults that would have shipped broken terrain.

Both were found by reading Merged Lands function by function rather than file
by file, and neither raised, crashed, or failed a test that existed at the
time. They are pinned here because the only signal either produced was in the
game.
"""

from __future__ import annotations

from array import array

import pytest

from wraithguard.land.cleaning import CellDigest, clean_landmass, differs_any, digest
from wraithguard.land.diff import (
    LandData,
    LandscapeLayers,
    diff_against_reference,
)
from wraithguard.land.landmass import Landmass, PluginRecords, build_reference
from wraithguard.land.pipeline import finish, inherit_reference_layers, merge_landmass
from wraithguard.land.seams import find_tears, repair_seams
from wraithguard.land.slope import limit_slopes
from wraithguard.land.textures import KnownTextures
from wraithguard.tes3fields.landscape import LAND_SIZE, TEXTURE_SIZE, WNAM_SIZE

VERTICES = LAND_SIZE * LAND_SIZE

#: Exterior cell coordinates.
Coords = tuple[int, int]


def flat(value: int = 0, count: int = VERTICES) -> list[int]:
    """A flat grid of one repeated value.

    Args:
        value: The value.
        count: How many entries.

    Returns:
        The grid.
    """
    return [value] * count


class TestUndeclaredLayersAreIgnored:
    """A layer the record's flags do not declare is not terrain.

    ``DATA`` says which grids a ``LAND`` record uses and the engine ignores the
    rest -- "If the relevant bit isn't set, the related fields will not be
    loaded, even if present" (UESP). tes3conv emits every grid regardless, so
    an undeclared one arrives full of zeros.

    Diffing it reads as *this mod flattened the terrain and painted it black*.
    Measured on 290 real landscape records: 21 carry texture data the flags do
    not declare, 20 carry vertex colours, 6 carry heights.
    """

    def _layers(self, declared: LandData, **grids: list[int] | None) -> LandscapeLayers:
        """Build layers with an explicit declaration."""
        return LandscapeLayers(coords=(0, 0), declared=declared, **grids)

    def test_an_undeclared_colour_grid_is_not_a_change(self) -> None:
        """The zeros tes3conv emits must not read as a black cell."""
        reference = self._layers(
            LandData.VERTEX_HEIGHTS | LandData.VERTEX_COLORS,
            heights=flat(100),
            colors=flat(128, VERTICES * 3),
        )
        plugin = self._layers(
            LandData.VERTEX_HEIGHTS | LandData.VERTEX_NORMALS,
            heights=flat(100),
            colors=flat(0, VERTICES * 3),
        )
        result = diff_against_reference("mod.esp", plugin, reference)
        assert result.colors is None
        assert not result.modified_data & LandData.VERTEX_COLORS

    def test_an_undeclared_texture_grid_is_not_a_change(self) -> None:
        """The same for texture indices, the most common case in the sample."""
        reference = self._layers(
            LandData.VERTEX_HEIGHTS | LandData.TEXTURES,
            heights=flat(100),
            textures=flat(4, TEXTURE_SIZE * TEXTURE_SIZE),
        )
        plugin = self._layers(
            LandData.VERTEX_HEIGHTS | LandData.VERTEX_NORMALS,
            heights=flat(100),
            textures=flat(0, TEXTURE_SIZE * TEXTURE_SIZE),
        )
        assert diff_against_reference("mod.esp", plugin, reference).textures is None

    def test_a_declared_layer_is_still_diffed(self) -> None:
        """The guard must not suppress real edits."""
        reference = self._layers(
            LandData.VERTEX_HEIGHTS | LandData.VERTEX_COLORS,
            heights=flat(100),
            colors=flat(128, VERTICES * 3),
        )
        plugin = self._layers(
            LandData.VERTEX_HEIGHTS | LandData.VERTEX_COLORS,
            heights=flat(100),
            colors=flat(0, VERTICES * 3),
        )
        assert diff_against_reference("mod.esp", plugin, reference).colors is not None

    def test_the_caller_cannot_opt_into_an_undeclared_layer(self) -> None:
        """``allowed`` narrows what is merged; it cannot widen it."""
        reference = self._layers(LandData.VERTEX_HEIGHTS, heights=flat(100))
        plugin = self._layers(
            LandData.VERTEX_HEIGHTS,
            heights=flat(100),
            textures=flat(9, TEXTURE_SIZE * TEXTURE_SIZE),
        )
        result = diff_against_reference("mod.esp", plugin, reference, allowed=~LandData.NONE)
        assert result.textures is None


class TestUnchangedLayersSurvive:
    """A merged record replaces the whole record, not the layers it changed.

    If a mod reshapes a cell's heights and nobody touches its textures, a
    merged record carrying heights alone leaves that cell with *no* texture
    data -- the mod's and the game's are both gone, because the record holding
    them has been superseded. The terrain renders untextured and nothing in the
    merge reports it.

    Measured before the fix: of 24 cells written from two Solstheim mods, 13
    lost textures the reference had, 14 lost vertex colours, 12 lost the world
    map.
    """

    def _reference(self) -> Landmass:
        """A one-cell reference landmass carrying every layer."""
        record = {
            "type": "Landscape",
            "grid": [0, 0],
            "landscape_flags": (
                "USES_VERTEX_HEIGHTS_AND_NORMALS | USES_VERTEX_COLORS | USES_TEXTURES"
            ),
        }
        landmass, _ = build_reference([PluginRecords("Morrowind.esm", [record])])
        layers = landmass.cells[(0, 0)]
        layers.heights = flat(100)
        layers.textures = flat(4, TEXTURE_SIZE * TEXTURE_SIZE)
        layers.colors = flat(128, VERTICES * 3)
        layers.world_map = flat(7, WNAM_SIZE * WNAM_SIZE)
        return landmass

    def test_unchanged_layers_are_inherited(self) -> None:
        """A cell whose heights moved keeps the reference's other layers."""
        from wraithguard.land.pipeline import MergedCell

        cell = MergedCell(coords=(0, 0), heights=array("i", flat(200)))
        inherit_reference_layers(cell, self._reference())
        assert cell.textures == flat(4, TEXTURE_SIZE * TEXTURE_SIZE)
        assert cell.colors == flat(128, VERTICES * 3)
        assert cell.world_map == flat(7, WNAM_SIZE * WNAM_SIZE)

    def test_merged_layers_are_not_overwritten(self) -> None:
        """Inheriting must not clobber what the merge actually decided."""
        from wraithguard.land.pipeline import MergedCell

        cell = MergedCell(
            coords=(0, 0),
            heights=array("i", flat(200)),
            textures=flat(9, TEXTURE_SIZE * TEXTURE_SIZE),
        )
        inherit_reference_layers(cell, self._reference())
        assert cell.textures == flat(9, TEXTURE_SIZE * TEXTURE_SIZE)

    def test_heights_are_inherited_too(self) -> None:
        """A cell edited only for textures still needs its terrain."""
        from wraithguard.land.pipeline import MergedCell

        cell = MergedCell(coords=(0, 0), textures=flat(9, TEXTURE_SIZE * TEXTURE_SIZE))
        inherit_reference_layers(cell, self._reference())
        assert cell.heights is not None
        assert list(cell.heights) == flat(100)

    def test_new_land_inherits_nothing(self) -> None:
        """A cell the masters never had has no terrain to keep."""
        from wraithguard.land.pipeline import MergedCell

        cell = MergedCell(coords=(99, 99), heights=array("i", flat(50)))
        inherit_reference_layers(cell, self._reference())
        assert cell.textures is None
        assert cell.colors is None

    def test_a_real_merge_keeps_every_reference_layer(self) -> None:
        """End to end: nothing the reference had is silently dropped."""
        reference = self._reference()
        edited = {
            "type": "Landscape",
            "grid": [0, 0],
            "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
        }
        outcome = merge_landmass(reference, [PluginRecords("mod.esp", [edited])], KnownTextures())
        for coords, cell in outcome.cells.items():
            layers = reference.get(coords)
            assert layers is not None
            for name in ("heights", "textures", "colors", "world_map"):
                if getattr(layers, name) is not None:
                    assert getattr(cell, name) is not None, f"{name} dropped at {coords}"


@pytest.mark.parametrize(
    ("declared", "expected"),
    [
        ("USES_VERTEX_HEIGHTS_AND_NORMALS", True),
        ("USES_TEXTURES", True),
        ("", False),
    ],
)
def test_world_map_follows_any_named_flag(declared: str, expected: bool) -> None:
    """``WNAM`` has no flag; tes3 derives it from the other three.

    Args:
        declared: The record's flag string.
        expected: Whether a world map should be considered declared.
    """
    from wraithguard.land.diff import parse_landscape_flags

    assert bool(parse_landscape_flags(declared) & LandData.WORLD_MAP) is expected


class TestMastersCombinePerLayer:
    """A later master replaces the layers it declares, not the whole record.

    ``merge_tes3_landscape`` starts from the earlier master and overwrites only
    the layers the later one both declares and carries, unioning the flags. A
    master that redefines a cell with heights alone therefore leaves the earlier
    master's vertex colours and textures standing.

    The vanilla three never overlap -- Tribunal has no landscape and Bloodmoon
    is Solstheim -- so this is invisible on a stock install and matters as soon
    as ``Tamriel_Data.esm``, ``OAAB_Data.esm`` or any other master-flagged
    expansion is present.
    """

    def _record(self, flags: str) -> dict[str, object]:
        """A landscape record declaring ``flags`` at cell (0, 0)."""
        return {"type": "Landscape", "grid": [0, 0], "landscape_flags": flags}

    def test_a_later_master_keeps_earlier_layers(self) -> None:
        """Heights-only from the second master must not erase the first's."""
        landmass, _ = build_reference(
            [
                PluginRecords(
                    "A.esm",
                    [
                        self._record(
                            "USES_VERTEX_HEIGHTS_AND_NORMALS | USES_VERTEX_COLORS "
                            "| USES_TEXTURES"
                        )
                    ],
                ),
                PluginRecords("B.esm", [self._record("USES_VERTEX_HEIGHTS_AND_NORMALS")]),
            ]
        )
        declared = landmass.cells[(0, 0)].declared
        assert declared & LandData.VERTEX_COLORS
        assert declared & LandData.TEXTURES

    def test_the_later_master_still_wins_its_own_layers(self) -> None:
        """Where both declare a layer, the later one supplies it."""
        first = self._record("USES_TEXTURES")
        second = self._record("USES_TEXTURES")
        landmass, _ = build_reference(
            [PluginRecords("A.esm", [first]), PluginRecords("B.esm", [second])]
        )
        assert landmass.sources[(0, 0)] == "B.esm"

    def test_an_undeclared_layer_is_not_taken(self) -> None:
        """A grid the later master does not declare is zeros, not terrain."""
        from wraithguard.land.landmass import merge_master_layers

        existing = LandscapeLayers(
            coords=(0, 0),
            declared=LandData.TEXTURES,
            textures=flat(4, TEXTURE_SIZE * TEXTURE_SIZE),
        )
        incoming = LandscapeLayers(
            coords=(0, 0),
            declared=LandData.VERTEX_HEIGHTS,
            textures=flat(0, TEXTURE_SIZE * TEXTURE_SIZE),
        )
        merge_master_layers(existing, incoming)
        assert existing.textures == flat(4, TEXTURE_SIZE * TEXTURE_SIZE)

    def test_a_declared_but_absent_layer_is_not_taken(self) -> None:
        """Nothing to copy is not the same as copying nothing."""
        from wraithguard.land.landmass import merge_master_layers

        existing = LandscapeLayers(
            coords=(0, 0), declared=LandData.VERTEX_HEIGHTS, heights=flat(100)
        )
        incoming = LandscapeLayers(coords=(0, 0), declared=LandData.VERTEX_HEIGHTS, heights=None)
        merge_master_layers(existing, incoming)
        assert existing.heights == flat(100)


class TestNormalsFollowTheHeights:
    """Normals are recomputed where terrain moved and inherited where it did not.

    Normals light the surface, so a cell whose heights changed but whose
    normals did not is lit as though the old ground were still there. Blanket
    recomputation fixes that and is slightly lossy the other way: a mod may
    hand-author a normal to fake a lighting effect its geometry does not
    produce. ``recompute_vertex_normals`` keeps the original wherever the
    height is unchanged, and so does :func:`resolve_normals`.
    """

    def _outcome(self, moved: bool) -> tuple[object, Landmass]:
        """A one-cell merge whose heights either moved or did not."""
        from wraithguard.land.pipeline import MergedCell, MergeOutcome

        record = {
            "type": "Landscape",
            "grid": [0, 0],
            "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
        }
        reference, _ = build_reference([PluginRecords("Morrowind.esm", [record])])
        layers = reference.cells[(0, 0)]
        layers.heights = flat(0)
        # A deliberately impossible normal: flat ground cannot produce it, so
        # its survival proves inheritance rather than coincidence.
        layers.normals = [99, -99, 42] * VERTICES

        outcome = MergeOutcome()
        outcome.cells[(0, 0)] = MergedCell(
            coords=(0, 0), heights=array("i", flat(500 if moved else 0))
        )
        return outcome, reference

    def test_an_unmoved_vertex_keeps_its_authored_normal(self) -> None:
        """Nothing moved, so nothing about the lighting should change."""
        from wraithguard.land.pipeline import resolve_normals

        outcome, reference = self._outcome(moved=False)
        preserved = resolve_normals(outcome, reference)
        normals = outcome.cells[(0, 0)].normals
        assert normals is not None
        assert normals[:3] == [99, -99, 42]
        assert preserved == VERTICES

    def test_a_moved_vertex_gets_a_fresh_normal(self) -> None:
        """Moved terrain must not keep normals describing the old shape."""
        from wraithguard.land.pipeline import resolve_normals

        outcome, reference = self._outcome(moved=True)
        resolve_normals(outcome, reference)
        normals = outcome.cells[(0, 0)].normals
        assert normals is not None
        assert normals[:3] != [99, -99, 42]

    def test_new_land_is_computed_from_its_own_heights(self) -> None:
        """A cell with no reference has no normals to inherit."""
        from wraithguard.land.pipeline import MergedCell, resolve_normals

        outcome, reference = self._outcome(moved=False)
        outcome.cells[(9, 9)] = MergedCell(coords=(9, 9), heights=array("i", flat(0)))
        resolve_normals(outcome, reference)
        normals = outcome.cells[(9, 9)].normals
        assert normals is not None
        # Flat ground points straight up.
        assert normals[:3] == [0, 0, 127]

    def test_every_normal_points_upward(self) -> None:
        """A downward normal means the surface is lit from underneath."""
        from wraithguard.land.pipeline import resolve_normals

        outcome, reference = self._outcome(moved=True)
        resolve_normals(outcome, reference)
        normals = outcome.cells[(0, 0)].normals
        assert normals is not None
        assert all(normals[i + 2] > 0 for i in range(0, len(normals), 3))


class TestNoTornBorders:
    """Nothing the merge writes may disagree with the ground beside it.

    Three distinct faults produced tears, each fixed by a different rule, and
    each fix exposed the next:

    1. Only *modified* cells were merged, so a merged cell's border with
       untouched vanilla was never reconciled -- 16 borders, worst 5,024 units.
       Fixed by borrowing reference neighbours.
    2. Corners are shared by *four* cells and only orthogonal neighbours were
       borrowed, so the diagonal was absent. Fixed by borrowing diagonally.
    3. Where a sharing cell genuinely is not written, averaging the rest moves
       our side and not theirs. Fixed by anchoring to the reference height --
       the ground that is not going to move decides.

    Pinning all four instead was tried and is worse: the cells we *do* write
    then keep three different values and tear against each other (4 borders,
    worst 3,944 units).
    """

    def _grid(self, value: int) -> array[int]:
        """A flat cell at one height."""
        return array("i", flat(value))

    def _borders(self, cells: dict[tuple[int, int], array[int]]) -> list[int]:
        """Every disagreement across every shared border, in world units."""
        gaps: list[int] = []
        for (x, y), grid in cells.items():
            for dx, dy in ((1, 0), (0, 1)):
                other = cells.get((x + dx, y + dy))
                if other is None:
                    continue
                pairs = (
                    [(i * LAND_SIZE + LAND_SIZE - 1, i * LAND_SIZE) for i in range(LAND_SIZE)]
                    if dx
                    else [((LAND_SIZE - 1) * LAND_SIZE + i, i) for i in range(LAND_SIZE)]
                )
                gaps.extend(abs(grid[a] - other[b]) for a, b in pairs if grid[a] != other[b])
        return gaps

    def test_a_full_block_is_reconciled(self) -> None:
        """Four cells meeting at a corner all agree afterwards."""
        from wraithguard.land.seams import repair_seams

        cells = {
            (0, 0): self._grid(10),
            (1, 0): self._grid(20),
            (0, 1): self._grid(30),
            (1, 1): self._grid(40),
        }
        repair_seams(cells)
        assert self._borders(cells) == []

    def test_an_incomplete_block_still_agrees_internally(self) -> None:
        """Three cells of a quad must not be left disagreeing with each other.

        This is the case that pinning got wrong.
        """
        from wraithguard.land.seams import repair_seams

        cells = {
            (0, 0): self._grid(10),
            (1, 0): self._grid(20),
            (0, 1): self._grid(30),
        }
        repair_seams(cells)
        assert self._borders(cells) == []

    def test_an_absent_neighbour_anchors_the_corner(self) -> None:
        """The ground we are not writing decides, because it will not move."""
        from wraithguard.land.seams import repair_seams

        cells = {
            (0, 0): self._grid(10),
            (1, 0): self._grid(20),
            (0, 1): self._grid(30),
        }
        anchor = {(1, 1): self._grid(999)}
        report = repair_seams(cells, anchor=anchor)
        assert report.anchored_corners > 0
        # Every present cell adopted the absent cell's height at that corner.
        assert cells[(0, 0)][_corner_index()] == 999
        assert self._borders(cells) == []

    def test_a_seamless_landmass_is_left_alone(self) -> None:
        """Repair must not invent movement where there is no disagreement."""
        from wraithguard.land.seams import repair_seams

        cells = {(0, 0): self._grid(50), (1, 0): self._grid(50)}
        assert repair_seams(cells).total == 0


def _corner_index() -> int:
    """Flat index of the north-east corner vertex."""
    return (LAND_SIZE - 1) * LAND_SIZE + (LAND_SIZE - 1)


class TestCleaningJudgesEveryLayer:
    """A cell is dropped only when *no* layer differs, not when heights match.

    Merged Lands' ``has_any_difference`` tests heights, normals, world map,
    colours and textures. Judging on heights alone drops a merged record whose
    heights happen to equal the reference, and once that record is gone the
    load order resolves the cell by last-wins -- so if one mod repainted its
    textures and another recoloured its vertices, one of the two edits is
    silently lost. Heights are unchanged in that case, so nothing in the merge
    reports it and the only signal is in the game.
    """

    def _digest(self, **layers: int | None) -> CellDigest:
        """A cell digest built from one repeated value per named layer."""
        return CellDigest(
            **{name: (None if value is None else digest([value])) for name, value in layers.items()}
        )

    def test_a_colour_only_edit_is_not_dropped(self) -> None:
        """Matching heights must not condemn a cell whose colours moved."""
        merged = {(0, 0): self._digest(heights=100, colors=9)}
        reference = {(0, 0): self._digest(heights=100, colors=1)}
        keep, report = clean_landmass(merged, reference, {(0, 0): ["a.esp", "b.esp"]}, {})
        assert (0, 0) in keep
        assert report.unmodified == 0

    def test_a_texture_only_edit_is_not_dropped(self) -> None:
        """The same for textures, the layer heights say least about."""
        merged = {(0, 0): self._digest(heights=100, textures=7)}
        reference = {(0, 0): self._digest(heights=100, textures=3)}
        keep, _report = clean_landmass(merged, reference, {(0, 0): ["a.esp", "b.esp"]}, {})
        assert (0, 0) in keep

    def test_a_genuinely_unmodified_cell_is_still_dropped(self) -> None:
        """The check must not become a rubber stamp that keeps everything."""
        same = self._digest(heights=100, colors=1, textures=3, world_map=2)
        keep, report = clean_landmass({(0, 0): same}, {(0, 0): same}, {(0, 0): ["a.esp"]}, {})
        assert keep == set()
        assert report.unmodified == 1

    def test_a_single_source_cell_matching_its_mod_is_dropped(self) -> None:
        """The load order already delivers it; the record would be redundant."""
        mine = self._digest(heights=200, textures=7)
        keep, report = clean_landmass(
            {(0, 0): mine},
            {(0, 0): self._digest(heights=100, textures=3)},
            {(0, 0): ["a.esp"]},
            {(0, 0): mine},
        )
        assert keep == set()
        assert report.single_source == 1

    def test_a_single_source_cell_seam_repair_moved_is_kept(self) -> None:
        """Dropping it would bring back the tear it was repaired for."""
        keep, report = clean_landmass(
            {(0, 0): self._digest(heights=205, textures=7)},
            {(0, 0): self._digest(heights=100, textures=3)},
            {(0, 0): ["a.esp"]},
            {(0, 0): self._digest(heights=200, textures=7)},
        )
        assert (0, 0) in keep
        assert report.kept_for_seams == 1

    def test_a_layer_only_one_side_has_is_not_a_difference(self) -> None:
        """``has_difference`` returns false the moment either side is absent."""
        merged = self._digest(heights=100, colors=9)
        reference = self._digest(heights=100, colors=None)
        assert not differs_any(merged, reference)

    def test_new_land_is_never_dropped_as_unmodified(self) -> None:
        """A cell the masters never had has no reference to match."""
        keep, report = clean_landmass(
            {(9, 9): self._digest(heights=100)}, {}, {(9, 9): ["a.esp"]}, {}
        )
        assert (9, 9) in keep
        assert report.unmodified == 0

    def test_a_texture_only_cell_survives_a_real_merge(self) -> None:
        """End to end: heights equal to the reference, textures repainted."""
        record = {
            "type": "Landscape",
            "grid": [0, 0],
            "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS | USES_TEXTURES",
        }
        reference_landmass, _ = build_reference([PluginRecords("Morrowind.esm", [record])])
        layers = reference_landmass.cells[(0, 0)]
        layers.heights = flat(100)
        layers.textures = flat(4, TEXTURE_SIZE * TEXTURE_SIZE)

        first = dict(record)
        second = dict(record)
        outcome = merge_landmass(
            reference_landmass,
            [PluginRecords("a.esp", [first]), PluginRecords("b.esp", [second])],
            KnownTextures(),
        )
        assert outcome.cells, "the merge produced no cell at all"


class TestNothingTearsAtTheEnd:
    """The merge's post-condition: no border disagrees about shared ground.

    Merged Lands opens ``clean_landmass_diff`` with
    ``assert_eq!(repair_landmass_seams(landmass), 0)`` -- it repairs the seams,
    then repairs them again and requires the second pass to find nothing. We
    did not port that assertion, which left the one defect a player sees in the
    first minute with no check at all: a wall or a chasm along a cell boundary,
    in a game where cells are 8,192 units across and every boundary is
    somewhere you walk.

    It matters more here than in the original, because more runs after the
    repair: the slope limiter moves vertices, feathering moves vertices, and
    cleaning removes whole cells.
    """

    def _grid(self, value: int = 100) -> array[int]:
        """A flat cell of one height."""
        return array("i", [value] * VERTICES)

    def test_matching_borders_are_not_a_tear(self) -> None:
        """The check must not cry wolf on terrain that is already correct."""
        cells = {(0, 0): self._grid(), (1, 0): self._grid()}
        assert find_tears(cells) == []

    def test_a_disagreeing_border_is_found(self) -> None:
        """One vertex out of 65 is still a tear, and still visible."""
        left, right = self._grid(), self._grid()
        right[0] = 900
        tears = find_tears({(0, 0): left, (1, 0): right})
        assert len(tears) == 1
        assert tears[0].vertices == 1
        assert tears[0].worst == 800

    def test_a_border_with_unwritten_terrain_is_checked(self) -> None:
        """A written cell does not get to be merely self-consistent.

        The ground next door still exists, supplied by the master, and it is
        not going to move -- so a merged cell that disagrees with it tears
        against the world even though every cell being written agrees.
        """
        reference = {(0, 1): self._grid(500)}
        tears = find_tears({(0, 0): self._grid(100)}, reference)
        assert len(tears) == 1
        assert tears[0].right is None
        assert tears[0].vertices == LAND_SIZE

    def test_no_terrain_next_door_is_not_a_tear(self) -> None:
        """Off the edge of the world there is nothing to tear against."""
        assert find_tears({(0, 0): self._grid()}, {}) == []

    def test_worst_first(self) -> None:
        """A report a human reads should lead with the one that matters."""
        a, b, c = self._grid(), self._grid(), self._grid()
        b[0] = 300
        c[_last_row_start()] = 5000
        tears = find_tears({(0, 0): a, (1, 0): b, (0, 1): c})
        assert tears == sorted(tears, key=lambda tear: -tear.worst)

    def _torn_merge(self, repair: bool) -> int:
        """Run a merge that tears unless seam repair fixes it.

        A mod raises one cell 800 units above the reference. Its neighbour is
        not edited, so the only thing holding the border together is the
        repair.

        Args:
            repair: Whether to run seam repair.

        Returns:
            How many tears the post-condition found.
        """
        records = [
            {
                "type": "Landscape",
                "grid": [x, y],
                "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
            }
            for x, y in ((0, 0), (0, 1))
        ]
        reference, _ = build_reference([PluginRecords("Morrowind.esm", records)])
        reference.cells[(0, 0)].heights = flat(100)
        reference.cells[(0, 1)].heights = flat(100)
        outcome = merge_landmass(
            reference, [PluginRecords("mod.esp", [records[0]])], KnownTextures()
        )
        heights = outcome.cells[(0, 0)].heights
        assert heights is not None
        for index in range(len(heights)):
            heights[index] = 900
        finish(outcome, reference, repair=repair, clean=False, limit=False)
        return len(outcome.seams.tears)

    def test_the_check_is_not_vacuous(self) -> None:
        """With repair off, the tear it exists to catch must be caught.

        A post-condition that cannot fail is decoration. This is the negative
        control: the same merge, minus the one step that fixes it.
        """
        assert self._torn_merge(repair=False) == 1

    def test_repair_closes_the_tear_the_check_finds(self) -> None:
        """And with repair on, the same merge is clean."""
        assert self._torn_merge(repair=True) == 0

    def test_the_check_runs_even_without_cleaning(self) -> None:
        """``--no-clean`` is a diagnosis mode; skipping the check defeats it."""
        assert self._torn_merge(repair=True) == 0

    def test_a_real_merge_leaves_no_tears(self) -> None:
        """End to end, through repair, the limiter and cleaning."""
        record = {
            "type": "Landscape",
            "grid": [0, 0],
            "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
        }
        reference, _ = build_reference([PluginRecords("Morrowind.esm", [record])])
        reference.cells[(0, 0)].heights = flat(100)
        outcome = merge_landmass(reference, [PluginRecords("mod.esp", [record])], KnownTextures())
        finish(outcome, reference)
        assert outcome.seams.tears == []


def _last_row_start() -> int:
    """Index of the first vertex in a cell's last row.

    Returns:
        The flat index.
    """
    return (LAND_SIZE - 1) * LAND_SIZE


class TestBorrowedCellsAreNotMoved:
    """A cell borrowed to reconcile a border must not itself be edited.

    ``add_reference_neighbours`` brings in untouched vanilla cells so a merged
    cell's borders have something to agree with. Averaging the two moves the
    *vanilla* cell -- and the cell beyond it, which was not borrowed, still
    holds its original heights. The tear is not removed; it is relocated one
    cell further from the edit, and it is now a tear in terrain no mod asked to
    change.

    Measured on a real 27-master, 940-mod order before the fix: **783 borders
    still disagreeing, the worst by 17,560 units** -- roughly half the
    difference between a mod's terrain and vanilla's, deposited into vanilla.

    So a borrowed cell is authoritative. The merged side adopts its heights
    whole, it never changes, and cleaning drops it again as unmodified.
    """

    def _patch(self, side: int = 5, height: int = 100) -> tuple[Landmass, list[Coords]]:
        """A square of flat vanilla cells.

        Args:
            side: Cells across.
            height: The flat height.

        Returns:
            The reference landmass and its coordinates.
        """
        half = side // 2
        coords = [(x, y) for x in range(-half, half + 1) for y in range(-half, half + 1)]
        records = [
            {
                "type": "Landscape",
                "grid": [x, y],
                "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
            }
            for x, y in coords
        ]
        reference, _ = build_reference([PluginRecords("Morrowind.esm", records)])
        for point in coords:
            reference.cells[point].heights = flat(height)
        return reference, coords

    def _raise_middle(self, reference: Landmass, to: int = 900) -> object:
        """Merge one mod that raises the centre cell, then finish.

        Args:
            reference: The vanilla landmass.
            to: The height the mod sets.

        Returns:
            The merge outcome, after repair and the limiter.
        """
        record = {
            "type": "Landscape",
            "grid": [0, 0],
            "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
        }
        outcome = merge_landmass(reference, [PluginRecords("mod.esp", [record])], KnownTextures())
        heights = outcome.cells[(0, 0)].heights
        assert heights is not None
        for index in range(len(heights)):
            heights[index] = to
        finish(outcome, reference, clean=False)
        return outcome

    def test_a_raised_cell_leaves_no_tear(self) -> None:
        """The end-to-end case the real load order failed on."""
        reference, _coords = self._patch()
        assert self._raise_middle(reference).seams.tears == []

    def test_the_borrowed_cells_are_untouched(self) -> None:
        """If one moved, the cell beyond it now disagrees with it."""
        reference, _coords = self._patch()
        outcome = self._raise_middle(reference)
        moved = [
            point
            for point, cell in outcome.cells.items()
            if point != (0, 0) and cell.heights is not None and any(v != 100 for v in cell.heights)
        ]
        assert moved == []

    def test_the_merged_cell_adopts_the_vanilla_edge(self) -> None:
        """Not a midpoint. Vanilla is what the game has, so vanilla decides."""
        reference, _coords = self._patch()
        outcome = self._raise_middle(reference)
        heights = outcome.cells[(0, 0)].heights
        assert heights is not None
        assert heights[0] == 100

    def test_averaging_moves_the_borrowed_cell(self) -> None:
        """The negative control: the defect itself, with the guard removed.

        Repair with nothing marked authoritative -- the behaviour before the
        fix. The borrowed vanilla cell is dragged toward the mod's terrain,
        which is an edit to ground no mod asked to change and which the cell
        beyond it does not follow.
        """
        from array import array as make

        cells = {(0, 0): make("i", flat(900)), (1, 0): make("i", flat(100))}
        repair_seams(cells, anchor=None, authoritative=frozenset())
        assert any(v != 100 for v in cells[(1, 0)]), "vanilla should have been moved"

    def test_marking_it_authoritative_leaves_it_alone(self) -> None:
        """With the guard, vanilla is untouched and the mod's cell yields."""
        from array import array as make

        cells = {(0, 0): make("i", flat(900)), (1, 0): make("i", flat(100))}
        repair_seams(cells, anchor=None, authoritative=frozenset({(1, 0)}))
        assert all(v == 100 for v in cells[(1, 0)])
        assert cells[(0, 0)][LAND_SIZE - 1] == 100

    def test_the_slope_limiter_leaves_borrowed_cells_alone(self) -> None:
        """The limiter runs *after* the repair and can undo it.

        Seam repair honouring the guard is not enough. The limiter then sweeps
        the same grids looking for steps too steep for ``VHGT``, and a large
        mod-versus-vanilla difference gives it plenty to find. If it may move a
        borrowed cell's vertices it edits vanilla terrain and reopens exactly
        the seam the repair just closed.

        This is pinned separately because it was missed the first time: the
        guard was added to ``_is_movable`` but ``limit_slopes`` called it
        without passing ``authoritative``, so the check was inert. An
        800-unit difference was too small to expose it; 18,000 was not.
        """
        from array import array as make

        cells = {(0, 0): make("i", flat(18_000))}
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                if (dx, dy) != (0, 0):
                    cells[(dx, dy)] = make("i", flat(100))
        borrowed = frozenset(c for c in cells if c != (0, 0))
        beyond = {
            (x, y): make("i", flat(100))
            for x in range(-2, 3)
            for y in range(-2, 3)
            if (x, y) not in cells
        }

        repair_seams(cells, authoritative=borrowed)
        assert all(all(v == 100 for v in cells[c]) for c in borrowed), "repair moved vanilla"
        limit_slopes(cells, authoritative=borrowed)
        assert all(all(v == 100 for v in cells[c]) for c in borrowed), "the limiter moved vanilla"
        assert find_tears(cells, beyond) == []

    def test_a_large_difference_still_leaves_no_tear(self) -> None:
        """End to end at 18,000 units, the scale a real load order reaches."""
        reference, _coords = self._patch(side=7)
        assert self._raise_middle(reference, to=18_000).seams.tears == []

    def test_a_deeper_patch_still_leaves_no_tear(self) -> None:
        """Seven cells across, so the borrowed ring is not the outermost."""
        reference, _coords = self._patch(side=7)
        assert self._raise_middle(reference).seams.tears == []


class TestTheCheckDoesNotInventTears:
    """A dropped cell is not absent ground; it is ground from another file.

    ``_check_borders`` originally ran after cleaning and treated any neighbour
    that was not written as reference terrain. Cleaning drops a cell exactly
    when the load order already delivers that terrain -- either nothing edited
    it, or one mod did and its own record produces the same result. So the
    ground is still there; it just comes from a different plugin.

    Comparing the survivor against *vanilla* in that case measures a merged
    cell against terrain the mod replaced, and reports a tear the size of the
    mod's edit. On a real 940-mod order that was 758 borders, the worst
    17,560 units, for borders that were perfectly intact -- and it refused to
    write the plugin over it.
    """

    def test_a_single_mod_area_is_not_reported_as_torn(self) -> None:
        """Two adjacent cells, one mod, terrain far from vanilla.

        Both are single-source and identical to the mod's own records, so
        cleaning drops them. Nothing should be reported: the mod's own plugin
        puts exactly that terrain there.
        """
        records = [
            {
                "type": "Landscape",
                "grid": [x, 0],
                "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
            }
            for x in (0, 1)
        ]
        reference, _ = build_reference([PluginRecords("Morrowind.esm", records)])
        for x in (0, 1):
            reference.cells[(x, 0)].heights = flat(100)

        outcome = merge_landmass(reference, [PluginRecords("mod.esp", records)], KnownTextures())
        for point in ((0, 0), (1, 0)):
            heights = outcome.cells[point].heights
            assert heights is not None
            for index in range(len(heights)):
                heights[index] = 18_000

        finish(outcome, reference)
        assert outcome.seams.tears == [], "a tear was invented where the mod's own terrain sits"

    def test_a_real_tear_is_still_caught(self) -> None:
        """The check must not have been softened into uselessness.

        Same landmass, but seam repair is skipped, so the merged cell really
        does disagree with the vanilla around it.
        """
        record = {
            "type": "Landscape",
            "grid": [0, 0],
            "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
        }
        neighbours = [
            {
                "type": "Landscape",
                "grid": [x, y],
                "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS",
            }
            for x in (-1, 0, 1)
            for y in (-1, 0, 1)
        ]
        reference, _ = build_reference([PluginRecords("Morrowind.esm", neighbours)])
        for x in (-1, 0, 1):
            for y in (-1, 0, 1):
                reference.cells[(x, y)].heights = flat(100)

        outcome = merge_landmass(reference, [PluginRecords("mod.esp", [record])], KnownTextures())
        heights = outcome.cells[(0, 0)].heights
        assert heights is not None
        for index in range(len(heights)):
            heights[index] = 18_000

        finish(outcome, reference, repair=False, limit=False)
        assert outcome.seams.tears, "an unrepaired border should still be reported"


class TestVanillaSeamsAreNotOurs:
    """A border between two cells we never edit is not this tool's defect.

    Borrowed cells are present only so an edit beside them has something to
    agree with. ``repair_edges`` deliberately refuses to move either side of a
    border *they* share -- whatever they disagree about is the game's own, it
    predates the merge, and it will still be there whether or not the tool runs.

    Moving the post-condition before cleaning (fault 12) put those cells in
    front of it for the first time, and it reported them: **62 borders, the
    worst 2,648 units**, on terrain the merge does not write. Blaming the tool
    for the landscape it was pointed at, and refusing to write over it.
    """

    def _pair(self, left: int, right: int) -> dict[Coords, object]:
        """Two vertically adjacent flat cells at different heights."""
        from array import array as make

        return {(0, 0): make("i", flat(left)), (0, 1): make("i", flat(right))}

    def test_two_untouched_cells_are_not_reported(self) -> None:
        """The game's own seam, between two cells we will not write."""
        cells = self._pair(100, 2748)
        assert find_tears(cells, None, frozenset({(0, 0), (0, 1)})) == []

    def test_an_edit_against_untouched_terrain_is_still_reported(self) -> None:
        """The fault-11 case. Exempting these would defeat the whole check."""
        cells = self._pair(100, 2748)
        assert find_tears(cells, None, frozenset({(0, 1)}))

    def test_two_edited_cells_are_still_reported(self) -> None:
        """Nothing borrowed, so both are ours and both must agree."""
        assert find_tears(self._pair(100, 2748), None, frozenset())

    def test_a_borrowed_cell_is_not_checked_against_terrain_beyond_it(self) -> None:
        """Its far border is untouched on both sides for the same reason."""
        from array import array as make

        cells = {(0, 0): make("i", flat(100))}
        beyond = {(0, 1): make("i", flat(9000))}
        assert find_tears(cells, beyond, frozenset({(0, 0)})) == []
