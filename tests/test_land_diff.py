"""Tests for :mod:`wraithguard.land.diff`.

The property that matters is the one the merger is built on: two plugins that
moved *different* vertices must report disjoint change sets, and two that moved
the same vertex must report an overlap. Everything else is bookkeeping.
"""

from __future__ import annotations

import pytest

from wraithguard.land.diff import (
    LandData,
    LandscapeDiff,
    LandscapeLayers,
    RelativeGrid,
    diff_against_reference,
    parse_landscape_flags,
)


def flat(value: int = 0, side: int = 65, components: int = 1) -> list[int]:
    """Build a flat grid of one repeated value.

    Args:
        value: The value.
        side: Vertices per edge.
        components: Values per vertex.

    Returns:
        The flat grid.
    """
    return [value] * (side * side * components)


class TestParseFlags:
    """The record's flag field is text, and mods put odd things in it."""

    def test_heights_and_normals_share_one_record_flag(self) -> None:
        """One declared flag implies two layers, as the format defines it."""
        parsed = parse_landscape_flags("USES_VERTEX_HEIGHTS_AND_NORMALS")
        assert parsed & LandData.VERTEX_HEIGHTS
        assert parsed & LandData.VERTEX_NORMALS

    def test_several_flags_combine(self) -> None:
        """The field is names joined by a pipe."""
        parsed = parse_landscape_flags("USES_VERTEX_COLORS | USES_TEXTURES")
        assert parsed & LandData.VERTEX_COLORS
        assert parsed & LandData.TEXTURES

    def test_any_named_flag_implies_a_world_map(self) -> None:
        """WNAM has no flag of its own.

        tes3's ``uses_world_map_data()`` is a derived predicate -- true when any
        of the three named bits is set -- and the writer emits WNAM on that
        basis. Reading it any other way would disagree with the file.
        """
        assert parse_landscape_flags("USES_TEXTURES") & LandData.WORLD_MAP
        assert not parse_landscape_flags("") & LandData.WORLD_MAP

    def test_unknown_entries_are_ignored_not_fatal(self) -> None:
        """tes3conv emits bare hex for unnamed bits; the named ones still count."""
        parsed = parse_landscape_flags("USES_TEXTURES | 0x8")
        assert parsed & LandData.TEXTURES
        assert not parsed & LandData.VERTEX_HEIGHTS

    @pytest.mark.parametrize("value", [None, "", "   "])
    def test_absent_means_nothing_declared(self, value: str | None) -> None:
        """A missing field is not an error."""
        assert parse_landscape_flags(value) == LandData.NONE


class TestRelativeGrid:
    """A reference plus deltas, with a changed flag per vertex."""

    def test_length_is_validated(self) -> None:
        """A wrong-sized reference raises rather than indexing off the end."""
        with pytest.raises(ValueError, match="expected"):
            RelativeGrid([0, 1, 2], side=65)

    def test_identical_grids_report_no_change(self) -> None:
        """Equal terrain is not a modification."""
        grid = RelativeGrid.from_difference(flat(10), flat(10), side=65)
        assert not grid.is_modified
        assert grid.num_differences == 0

    def test_one_moved_vertex_is_found(self) -> None:
        """The change set names exactly the vertex that moved."""
        plugin = flat(10)
        plugin[7 * 65 + 3] = 99
        grid = RelativeGrid.from_difference(flat(10), plugin, side=65)
        assert grid.is_modified
        assert grid.num_differences == 1
        assert grid.changed_vertices() == [(3, 7)]
        assert grid.delta_at(3, 7) == 89
        assert grid.value_at(3, 7) == 99

    def test_a_multi_component_vertex_counts_once(self) -> None:
        """A normal whose three components all moved is one moved vertex."""
        plugin = flat(0, components=3)
        base = (2 * 65 + 5) * 3
        plugin[base], plugin[base + 1], plugin[base + 2] = 1, 2, 3
        grid = RelativeGrid.from_difference(flat(0, components=3), plugin, side=65, components=3)
        assert grid.num_differences == 1
        assert grid.changed_vertices() == [(5, 2)]

    def test_mismatched_plugin_length_is_refused(self) -> None:
        """Comparing grids of different sizes is a caller bug, not a diff."""
        with pytest.raises(ValueError, match="plugin grid"):
            RelativeGrid.from_difference(flat(0), flat(0)[:-1], side=65)

    def test_set_value_recomputes_the_flag(self) -> None:
        """Setting a vertex back to its reference clears its changed flag."""
        grid = RelativeGrid(flat(4), side=65)
        grid.set_value(1, 1, (9,))
        assert grid.has_difference(1, 1)
        grid.set_value(1, 1, (4,))
        assert not grid.has_difference(1, 1)

    def test_set_value_checks_component_count(self) -> None:
        """Supplying two components to a three-component grid raises."""
        grid = RelativeGrid(flat(0, components=3), side=65, components=3)
        with pytest.raises(ValueError, match="component"):
            grid.set_value(0, 0, (1, 2))

    def test_clear_discards_a_delta(self) -> None:
        """A cleared vertex returns to the reference value."""
        plugin = flat(0)
        plugin[0] = 50
        grid = RelativeGrid.from_difference(flat(0), plugin, side=65)
        grid.clear(0, 0)
        assert not grid.is_modified
        assert grid.value_at(0, 0) == 0

    def test_to_flat_reproduces_the_plugin_grid(self) -> None:
        """Reference plus delta is what the plugin had."""
        plugin = flat(3)
        plugin[100] = 77
        grid = RelativeGrid.from_difference(flat(3), plugin, side=65)
        assert grid.to_flat() == plugin

    def test_to_rows_refuses_multi_component_grids(self) -> None:
        """Rows of interleaved triples would be meaningless, so it raises."""
        grid = RelativeGrid(flat(0, components=3), side=65, components=3)
        with pytest.raises(ValueError, match="single-component"):
            grid.to_rows()


class TestDiffAgainstReference:
    """The per-plugin difference, which is what makes merging possible."""

    def _layers(
        self, heights: list[int] | None, coords: tuple[int, int] = (0, 0)
    ) -> LandscapeLayers:
        """Build layers carrying only heights."""
        return LandscapeLayers(
            coords=coords,
            declared=LandData.VERTEX_HEIGHTS,
            heights=heights,
        )

    def test_an_unchanged_plugin_reports_nothing(self) -> None:
        """A plugin that rewrote the record without changing it is not a change."""
        result = diff_against_reference("mod.esp", self._layers(flat(5)), self._layers(flat(5)))
        assert not result.is_modified
        assert result.modified_data == LandData.NONE
        assert result.heights is None

    def test_disjoint_edits_do_not_overlap(self) -> None:
        """The whole point: two mods touching different vertices can both survive."""
        reference = self._layers(flat(0))
        first, second = flat(0), flat(0)
        first[10] = 5
        second[900] = 7
        one = diff_against_reference("one.esp", self._layers(first), reference)
        two = diff_against_reference("two.esp", self._layers(second), reference)
        assert one.heights is not None
        assert two.heights is not None
        moved_one = set(one.heights.changed_vertices())
        moved_two = set(two.heights.changed_vertices())
        assert moved_one and moved_two
        assert not moved_one & moved_two

    def test_the_same_edit_overlaps(self) -> None:
        """Two mods moving one vertex contest it, and the diff says so."""
        reference = self._layers(flat(0))
        first, second = flat(0), flat(0)
        first[10] = 5
        second[10] = 9
        one = diff_against_reference("one.esp", self._layers(first), reference)
        two = diff_against_reference("two.esp", self._layers(second), reference)
        assert one.heights is not None
        assert two.heights is not None
        assert set(one.heights.changed_vertices()) == set(two.heights.changed_vertices())

    def test_new_land_has_no_reference(self) -> None:
        """A cell the masters lack is entirely new, so everything is a change."""
        plugin = flat(0)
        plugin[3] = 1
        result = diff_against_reference("new.esp", self._layers(plugin), None)
        assert result.is_modified
        assert result.modified_data == LandData.VERTEX_HEIGHTS

    def test_a_declined_layer_is_skipped(self) -> None:
        """``allowed`` lets a caller refuse to merge a layer at all."""
        reference = self._layers(flat(0))
        plugin = flat(0)
        plugin[10] = 5
        result = diff_against_reference(
            "mod.esp", self._layers(plugin), reference, allowed=LandData.TEXTURES
        )
        assert result.heights is None
        assert not result.is_modified

    def test_a_declared_but_absent_layer_is_reported(self) -> None:
        """A record claiming heights and carrying none is malformed, not empty."""
        result = diff_against_reference("broken.esp", self._layers(None), self._layers(flat(0)))
        assert "heights" in result.missing

    def test_modified_data_names_only_what_moved(self) -> None:
        """A plugin rewriting five layers but changing one reports the one."""
        reference = LandscapeLayers(
            coords=(0, 0),
            declared=LandData.VERTEX_HEIGHTS | LandData.VERTEX_COLORS,
            heights=flat(0),
            colors=flat(0, components=3),
        )
        moved = flat(0)
        moved[1] = 3
        plugin = LandscapeLayers(
            coords=(0, 0),
            declared=reference.declared,
            heights=moved,
            colors=flat(0, components=3),
        )
        result = diff_against_reference("mod.esp", plugin, reference)
        assert result.modified_data == LandData.VERTEX_HEIGHTS


class TestLandscapeLayersFromRecord:
    """Reading the record tes3conv writes."""

    def test_missing_coordinates_are_refused(self) -> None:
        """A landscape with no grid cannot be placed, so it raises."""
        with pytest.raises(ValueError, match="grid coordinates"):
            LandscapeLayers.from_record({"type": "Landscape"})

    def test_coordinates_are_read(self) -> None:
        """The grid pair becomes the cell's coordinates."""
        layers = LandscapeLayers.from_record({"type": "Landscape", "grid": [-3, 12]})
        assert layers.coords == (-3, 12)

    def test_absent_layers_stay_none(self) -> None:
        """None and all-zero are different claims and must not be conflated."""
        layers = LandscapeLayers.from_record({"type": "Landscape", "grid": [0, 0]})
        assert layers.heights is None
        assert layers.textures is None


class TestLandscapeDiffSummary:
    """The reporting properties."""

    def test_an_empty_diff_is_not_modified(self) -> None:
        """No grids means nothing changed."""
        assert not LandscapeDiff(coords=(0, 0), plugin="x.esp").is_modified

    def test_counts_add_across_layers(self) -> None:
        """The total is every moved vertex in every layer."""
        heights, colors = flat(0), flat(0, components=3)
        heights[1] = 1
        colors[3] = 1
        diff = LandscapeDiff(
            coords=(0, 0),
            plugin="x.esp",
            heights=RelativeGrid.from_difference(flat(0), heights, side=65),
            colors=RelativeGrid.from_difference(
                flat(0, components=3), colors, side=65, components=3
            ),
        )
        assert diff.num_differences == 2
