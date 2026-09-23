"""Tests for ``wraithguard.scene.cellview`` -- cell selection + orchestration.

Cells and plugins are built from the real records (``Cell``/``CellData``/
``Static``/``Reference``); the mesh loader is a fake, so no IO.
"""

from __future__ import annotations

from types import SimpleNamespace

from wraithguard.esp.flags import CellFlags
from wraithguard.esp.records.cell import Cell, CellData
from wraithguard.esp.records.reference import Reference
from wraithguard.esp.records.static_ import Static
from wraithguard.nif.geometry import Mesh
from wraithguard.scene.cellview import (
    CellKey,
    LoadedPlugin,
    cell_key,
    cell_label,
    cell_layers,
    list_cells,
    object_provenance,
    preview_cell,
    preview_cell_instanced,
)


def _interior(name: str, refs: list[Reference] | None = None) -> Cell:
    return Cell(
        name=name,
        data=CellData(cell_flags=CellFlags.IS_INTERIOR),
        references=refs or [],
    )


def _exterior(grid: tuple[int, int], region: str = "", refs: list[Reference] | None = None) -> Cell:
    return Cell(name="", region=region, data=CellData(grid=grid), references=refs or [])


class TestKeyAndLabel:
    def test_interior_key_is_the_lowercased_name(self) -> None:
        assert cell_key(_interior("Balmora, Guar Shack")) == CellKey(interior="balmora, guar shack")

    def test_exterior_key_is_the_grid(self) -> None:
        assert cell_key(_exterior((-4, -2))) == CellKey(grid=(-4, -2))

    def test_interior_label_is_the_name(self) -> None:
        assert cell_label(_interior("Balmora, Guar Shack")) == "Balmora, Guar Shack"

    def test_exterior_label_carries_region_and_grid(self) -> None:
        assert cell_label(_exterior((-4, -2), region="Ascadian Isles")) == "Ascadian Isles (-4, -2)"


class TestListCells:
    def test_it_lists_distinct_cells_with_the_plugins_that_touch_them(self) -> None:
        p1 = LoadedPlugin("A.esp", [], [_interior("Cave"), _exterior((1, 1))])
        p2 = LoadedPlugin("B.esp", [], [_interior("Cave")])  # also touches Cave
        choices = list_cells([p1, p2])
        by_label = {c.label: c for c in choices}
        assert set(by_label) == {"Cave", "Wilderness (1, 1)"}
        assert by_label["Cave"].plugins == ["A.esp", "B.esp"]

    def test_non_cell_records_are_ignored_and_a_plugin_is_listed_once(self) -> None:
        # A plugin carrying a non-Cell record (skipped) and defining the same cell
        # twice must appear once under that cell, not twice.
        plugin = LoadedPlugin(
            "A.esp", [], [Static(id="rock", mesh="r.nif"), _interior("Cave"), _interior("Cave")]
        )
        by_label = {c.label: c for c in list_cells([plugin])}
        assert set(by_label) == {"Cave"}
        assert by_label["Cave"].plugins == ["A.esp"]

    def test_interiors_sort_before_exteriors(self) -> None:
        p = LoadedPlugin("A.esp", [], [_exterior((5, 5)), _interior("Zzz"), _interior("Aaa")])
        labels = [c.label for c in list_cells([p])]
        assert labels == ["Aaa", "Zzz", "Wilderness (5, 5)"]


class TestCellLayers:
    def test_it_returns_the_matching_cell_or_none_per_plugin(self) -> None:
        cave_a = _interior("Cave")
        cave_b = _interior("Cave")
        plugins = [
            LoadedPlugin("A.esp", [], [cave_a]),
            LoadedPlugin("B.esp", [], [_interior("Other")]),  # different cell
            LoadedPlugin("C.esp", [], [cave_b]),
        ]
        layers = cell_layers(plugins, CellKey(interior="cave"))
        assert [name for name, _m, _c in layers] == ["A.esp", "B.esp", "C.esp"]
        assert [c for _n, _m, c in layers] == [cave_a, None, cave_b]


class TestPreviewCell:
    def test_end_to_end_resolves_and_builds(self) -> None:
        # A plugin defines a static with a mesh and places one reference to it.
        cell = _interior("Cave", refs=[Reference(id="rock", mast_index=0, refr_index=1)])
        plugin = LoadedPlugin("A.esp", [], [Static(id="rock", mesh="rock.nif"), cell])
        loaded: list[str] = []

        def load_mesh(path: str) -> list[Mesh]:
            loaded.append(path)
            return [Mesh(name="s", vertices=[(0.0, 0.0, 0.0)], triangles=[(0, 0, 0)])]

        placements, audit, scene = preview_cell([plugin], CellKey(interior="cave"), load_mesh)
        assert audit.references == 1 and audit.placed == 1
        assert [p.kind for p in placements] == ["placed"]
        assert loaded == ["rock.nif"] and scene.drawn == 1


def _rock_mesh(_path: str) -> list[Mesh]:
    return [Mesh(name="s", vertices=[(0.0, 0.0, 0.0)], triangles=[(0, 0, 0)])]


def _ref(obj_id: str, index: int, *, deleted: bool | None = None) -> Reference:
    return Reference(id=obj_id, mast_index=0, refr_index=index, deleted=deleted)


class TestObjectProvenance:
    def test_it_reports_definers_placers_and_the_winning_model(self) -> None:
        # rock defined in a base and overridden by a later plugin, placed by base.
        base = LoadedPlugin(
            "Base.esm",
            [],
            [Static(id="rock", mesh="rock.nif"), _exterior((0, 0), refs=[_ref("rock", 1)])],
        )
        over = LoadedPlugin("Over.esp", ["Base.esm"], [Static(id="rock", mesh="rock_hd.nif")])
        prov = object_provenance([base, over], CellKey(grid=(0, 0)))
        assert set(prov) == {"rock"}
        info = prov["rock"]
        assert info["id"] == "rock" and info["type"] == "Static"
        assert info["model"] == "rock_hd.nif"  # the later definer wins
        assert info["definedBy"] == ["Base.esm", "Over.esp"]  # load order
        assert info["placedBy"] == ["Base.esm"]

    def test_two_plugins_placing_the_same_object_are_both_listed(self) -> None:
        rock = Static(id="rock", mesh="rock.nif")
        p1 = LoadedPlugin("A.esp", [], [rock, _exterior((0, 0), refs=[_ref("rock", 1)])])
        p2 = LoadedPlugin("B.esp", [], [_exterior((0, 0), refs=[_ref("rock", 2)])])
        prov = object_provenance([p1, p2], CellKey(grid=(0, 0)))
        assert prov["rock"]["placedBy"] == ["A.esp", "B.esp"]

    def test_a_deleted_reference_does_not_count_as_placed(self) -> None:
        p = LoadedPlugin(
            "A.esp",
            [],
            [
                Static(id="rock", mesh="rock.nif"),
                _exterior((0, 0), refs=[_ref("rock", 1, deleted=True)]),
            ],
        )
        assert object_provenance([p], CellKey(grid=(0, 0))) == {}

    def test_idless_meshless_and_actor_records_are_left_out_of_definers(self) -> None:
        actor = SimpleNamespace(id="rat", mesh="rat.nif", TAG=b"CREA")  # actor: excluded
        idless = SimpleNamespace(id="", mesh="x.nif", TAG=b"STAT")  # no id: skipped
        marker = Static(id="mark", mesh="")  # no mesh: not a definer
        rock = Static(id="rock", mesh="rock.nif")
        cell = _exterior((0, 0), refs=[_ref("rat", 1), _ref("mark", 2), _ref("rock", 3)])
        prov = object_provenance(
            [LoadedPlugin("A.esp", [], [actor, idless, marker, rock, cell])], CellKey(grid=(0, 0))
        )
        assert prov["rock"]["definedBy"] == ["A.esp"]
        assert prov["rat"]["definedBy"] == [] and prov["rat"]["model"] == ""  # actor, no mesh
        assert prov["mark"]["definedBy"] == []  # meshless record is not a definer

    def test_a_plugin_that_defines_or_places_twice_is_listed_once(self) -> None:
        cell = _exterior((0, 0), refs=[_ref("rock", 1), _ref("rock", 2)])  # placed twice
        plugin = LoadedPlugin(
            "A.esp",
            [],
            [Static(id="rock", mesh="a.nif"), Static(id="rock", mesh="b.nif"), cell],
        )
        prov = object_provenance([plugin], CellKey(grid=(0, 0)))
        assert prov["rock"]["definedBy"] == ["A.esp"]  # defined twice, listed once
        assert prov["rock"]["placedBy"] == ["A.esp"]  # placed twice, listed once

    def test_a_plugin_not_touching_the_cell_contributes_nothing(self) -> None:
        toucher = LoadedPlugin(
            "A.esp",
            [],
            [Static(id="rock", mesh="rock.nif"), _exterior((0, 0), refs=[_ref("rock", 1)])],
        )
        bystander = LoadedPlugin("B.esp", [], [Static(id="tree", mesh="tree.nif")])  # no cell
        prov = object_provenance([toucher, bystander], CellKey(grid=(0, 0)))
        assert set(prov) == {"rock"}


class TestAdjacentCells:
    def _plugin_with_grid_and_neighbours(self) -> LoadedPlugin:
        """A plugin whose centre cell (0, 0) and every neighbour place one rock."""
        cells = [
            _exterior((gx, gy), refs=[Reference(id="rock", mast_index=0, refr_index=1)])
            for gx in (-1, 0, 1)
            for gy in (-1, 0, 1)
        ]
        records: list[object] = [Static(id="rock", mesh="rock.nif"), *cells]
        return LoadedPlugin("A.esp", [], records)

    def test_off_by_default_draws_only_the_picked_cell(self) -> None:
        plugin = self._plugin_with_grid_and_neighbours()
        _placements, _audit, cell = preview_cell_instanced(
            [plugin], CellKey(grid=(0, 0)), _rock_mesh
        )
        assert cell.drawn == 1
        assert all(not group.adjacent for group in cell.groups)

    def test_include_adjacent_pulls_in_the_eight_neighbours(self) -> None:
        plugin = self._plugin_with_grid_and_neighbours()
        placements, audit, cell = preview_cell_instanced(
            [plugin], CellKey(grid=(0, 0)), _rock_mesh, include_adjacent=True
        )
        # The picked cell's placements and audit are its own -- neighbours are
        # context, not part of what the load order does to this cell.
        assert audit.references == 1
        assert len(placements) == 1
        # One rock in the centre plus one in each of the eight neighbours.
        assert cell.drawn == 9
        adjacent = [group for group in cell.groups if group.adjacent]
        picked = [group for group in cell.groups if not group.adjacent]
        assert len(adjacent) == 8
        assert len(picked) == 1

    def test_a_missing_neighbour_model_is_recorded_once(self) -> None:
        # The centre places a model that loads; every neighbour places one that
        # does not. The missing neighbour model must be listed once, not per cell.
        cells = [
            _exterior(
                (gx, gy),
                refs=[Reference(id="c" if (gx, gy) == (0, 0) else "e", mast_index=0, refr_index=1)],
            )
            for gx in (-1, 0, 1)
            for gy in (-1, 0, 1)
        ]
        records: list[object] = [
            Static(id="c", mesh="center.nif"),
            Static(id="e", mesh="edge.nif"),
            *cells,
        ]
        plugin = LoadedPlugin("A.esp", [], records)

        def loader(path: str) -> list[Mesh] | None:
            return (
                [Mesh(name="m", vertices=[(0.0, 0.0, 0.0)], triangles=[(0, 0, 0)])]
                if path == "center.nif"
                else None
            )

        _placements, _audit, cell = preview_cell_instanced(
            [plugin], CellKey(grid=(0, 0)), loader, include_adjacent=True
        )
        assert cell.missing_models == ["edge.nif"]  # the eight neighbours share one entry

    def test_interior_ignores_include_adjacent(self) -> None:
        cell_rec = _interior("Cave", refs=[Reference(id="rock", mast_index=0, refr_index=1)])
        plugin = LoadedPlugin("A.esp", [], [Static(id="rock", mesh="rock.nif"), cell_rec])
        _placements, _audit, cell = preview_cell_instanced(
            [plugin], CellKey(interior="cave"), _rock_mesh, include_adjacent=True
        )
        assert cell.drawn == 1
        assert all(not group.adjacent for group in cell.groups)
