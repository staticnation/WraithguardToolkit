"""Golden tests for :mod:`wraithguard.merge` against merge_to_master's fixtures.

Each fixture is a ``Master.esm`` / ``Plugin.esp`` / ``Expect.esm`` triple produced
by the original tool. We merge the plugin into the master and check the result
matches ``Expect.esm`` object-for-object. Comparison is structural rather than
byte-exact, so it does not depend on reproducing the source tool's record
ordering -- only on merging to the same content.

Fixtures whose plugin lists a master not bundled with the fixture (the vanilla
game masters) are skipped: the source project gitignores those large files.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from wraithguard.esp.plugin import read_header, read_plugin
from wraithguard.merge import MergeOptions, merge_plugins
from wraithguard.merge.masters import get_index_remap
from wraithguard.merge.model import PluginData

_ASSETS = Path(__file__).resolve().parents[1].parent / "merge_to_master-main" / "tests" / "assets"

_DEFAULT = MergeOptions()
_REMOVE_DELETED = MergeOptions(remove_deleted=True)

#: Fixture name -> the options the source tool's own test uses for it.
_FIXTURES: dict[str, MergeOptions] = {
    "info_insert_empty": _DEFAULT,
    "info_insert_front": _DEFAULT,
    "info_insert_middle": _DEFAULT,
    "info_insert_end": _DEFAULT,
    "info_insert_replacing": _DEFAULT,
    "info_preserve_gaps": _DEFAULT,
    "info_delete_front": _REMOVE_DELETED,
    "info_delete_end": _REMOVE_DELETED,
    "info_delete_middle": _REMOVE_DELETED,
    "info_delete_middle_2": _REMOVE_DELETED,
    "remove_deleted": _REMOVE_DELETED,
    "remove_deleted_fields": _REMOVE_DELETED,
    "remove_deleted_references": _REMOVE_DELETED,
    "rename_cells": _REMOVE_DELETED,
}


def _fixture_runnable(base: Path) -> bool:
    """Whether every master the fixture's plugin needs is present in its folder."""
    header = read_header((base / "Plugin.esp").read_bytes())
    return all((base / name).exists() for name, _ in header.masters)


def _refs_by_key(cell) -> dict[tuple[int, int], object]:
    """A cell's references keyed by ``(master index, object index)``."""
    return {(r.mast_index, r.refr_index): r for r in cell.references}


def _assert_cell_equal(got, want, where: str) -> None:
    """Assert two cells match on header fields and reference set."""
    assert got.flags == want.flags, f"{where}: cell flags"
    assert got.name == want.name, f"{where}: cell name"
    assert got.data == want.data, f"{where}: cell data"
    assert got.region == want.region, f"{where}: cell region"
    assert got.map_color == want.map_color, f"{where}: cell map_color"
    assert got.water_height == want.water_height, f"{where}: cell water_height"
    assert got.atmosphere_data == want.atmosphere_data, f"{where}: cell atmosphere"
    assert _refs_by_key(got) == _refs_by_key(want), f"{where}: references"


def _assert_equal(got: PluginData, want: PluginData) -> None:
    """Assert two merged plugins are structurally equal."""
    assert got.header.masters == want.header.masters, "header masters"
    assert got.header.file_type == want.header.file_type, "header file_type"

    assert set(got.objects) == set(want.objects), "object keys"
    for key, obj in got.objects.items():
        assert obj == want.objects[key], f"object {key!r}"

    assert set(got.cells.exteriors) == set(want.cells.exteriors), "exterior coords"
    for coords, ext in got.cells.exteriors.items():
        want_ext = want.cells.exteriors[coords]
        if ext.cell is not None or want_ext.cell is not None:
            assert ext.cell is not None and want_ext.cell is not None, f"exterior {coords} cell"
            _assert_cell_equal(ext.cell, want_ext.cell, f"exterior {coords}")
        assert ext.landscape == want_ext.landscape, f"exterior {coords} landscape"
        assert ext.pathgrid == want_ext.pathgrid, f"exterior {coords} pathgrid"

    assert set(got.cells.interiors) == set(want.cells.interiors), "interior names"
    for name, intr in got.cells.interiors.items():
        want_int = want.cells.interiors[name]
        if intr.cell is not None or want_int.cell is not None:
            assert intr.cell is not None and want_int.cell is not None, f"interior {name} cell"
            _assert_cell_equal(intr.cell, want_int.cell, f"interior {name}")
        assert intr.pathgrid == want_int.pathgrid, f"interior {name} pathgrid"

    assert set(got.dialogues) == set(want.dialogues), "dialogue topics"
    for topic, group in got.dialogues.items():
        want_group = want.dialogues[topic]
        assert group.dialogue == want_group.dialogue, f"dialogue {topic}"
        assert group.infos == want_group.infos, f"dialogue {topic} infos (order-sensitive)"


@pytest.mark.parametrize("name", sorted(_FIXTURES))
def test_merge_matches_expected(name: str) -> None:
    """Merging the fixture's plugin into its master reproduces ``Expect.esm``."""
    base = _ASSETS / name
    if not base.exists():
        pytest.skip(f"fixture {name} not available")
    if not _fixture_runnable(base):
        pytest.skip(f"fixture {name} needs masters not bundled with it")

    merged = merge_plugins(base / "Plugin.esp", base / "Master.esm", _FIXTURES[name])
    expect = PluginData.from_records(read_plugin((base / "Expect.esm").read_bytes()))
    _assert_equal(merged, expect)


def _names(entries: list[tuple[str, int]]) -> list[str]:
    """The master names from a list of ``(name, size)`` entries."""
    return [name for name, _ in entries]


def _masters(names: list[str]) -> list[tuple[str, int]]:
    """A master list of ``(name, 0)`` entries from names."""
    return [(name, 0) for name in names]


def test_index_remap_identical_masters_is_a_noop() -> None:
    """Identical master lists need no remap."""
    new, indices = get_index_remap(_masters(["A", "B", "C"]), _masters(["A", "B", "C"]), "")
    assert new is None and indices is None


def test_index_remap_appends_mismatched_masters() -> None:
    """A plugin master absent from the target is appended, and its index shifts."""
    new, indices = get_index_remap(_masters(["A"]), _masters(["B"]), "")
    assert new is not None and _names(new) == ["B", "A"]
    assert indices == [0, 2]


def test_index_remap_target_master_becomes_local() -> None:
    """The merge target is dropped from the list and remapped to local (0)."""
    new, indices = get_index_remap(_masters(["A", "B", "C"]), _masters(["A", "D"]), "B")
    assert new is not None and _names(new) == ["A", "D", "C"]
    assert indices == [0, 1, 0, 3]


def test_index_remap_same_masters_different_order() -> None:
    """Reordered masters keep the list but remap indices."""
    new, indices = get_index_remap(_masters(["C", "A", "B"]), _masters(["A", "C", "B"]), "")
    assert new is None
    assert indices == [0, 2, 1, 3]
