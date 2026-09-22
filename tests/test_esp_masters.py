"""Removing a master from a plugin and renumbering its references.

Covers the index arithmetic (a removed master shifts every later master down one,
references into the removed master are dropped, local and earlier references are
left alone), the no-op paths (master absent, no header), moved references, the
per-cell drop report, and a full round-trip through the plugin writer.
"""

from __future__ import annotations

import pytest

from wraithguard.esp import (
    read_plugin,
    remove_master,
    remove_master_from_bytes,
    rename_master,
    rename_master_in_bytes,
    write_plugin,
)
from wraithguard.esp.flags import CellFlags, ObjectFlags
from wraithguard.esp.records.cell import Cell, CellData
from wraithguard.esp.records.header import Header
from wraithguard.esp.records.reference import Reference
from wraithguard.esp.records.static_ import Static


def _interior(name: str, references: list[Reference]) -> Cell:
    """An interior cell carrying the given references."""
    return Cell(
        flags=ObjectFlags(0),
        name=name,
        data=CellData(cell_flags=CellFlags.IS_INTERIOR, grid=(0, 0)),
        references=references,
    )


def _exterior(coords: tuple[int, int], references: list[Reference]) -> Cell:
    """An exterior cell at ``coords`` carrying the given references."""
    return Cell(
        flags=ObjectFlags(0),
        name="",
        data=CellData(cell_flags=CellFlags(0), grid=coords),
        references=references,
    )


def _ref(id_: str, mast: int, refr: int = 1, **kw: object) -> Reference:
    """A reference to ``id_`` defined by master index ``mast``."""
    return Reference(mast_index=mast, refr_index=refr, id=id_, **kw)


def _header(*masters: str) -> Header:
    """A plugin header listing ``masters`` (each given a nominal size)."""
    return Header(masters=[(name, 1) for name in masters])


def test_remove_middle_master_shifts_later_indices() -> None:
    """Removing B from [A, B, C] drops refs into B and shifts C's refs down."""
    cell = _interior(
        "Hall",
        [
            _ref("local", 0),
            _ref("from_a", 1),
            _ref("from_b", 2),  # into B: dropped
            _ref("from_c", 3),  # into C: shifts to 2
        ],
    )
    records = [_header("A.esm", "B.esm", "C.esm"), cell]

    report = remove_master(records, "B.esm")

    assert report.removed is True
    assert report.master == "B.esm"
    assert report.remaining_masters == ["A.esm", "C.esm"]
    assert report.references_dropped == 1
    assert report.references_remapped == 1
    assert report.dropped_by_cell == {"Hall": 1}
    assert [(r.id, r.mast_index) for r in cell.references] == [
        ("local", 0),
        ("from_a", 1),
        ("from_c", 2),
    ]


def test_remove_spurious_master_drops_nothing() -> None:
    """The Better Bodies case: a listed master with no references just goes."""
    cell = _interior("Room", [_ref("local", 0), _ref("from_a", 1)])
    records = [_header("A.esm", "BetterBodies.esp"), cell]

    report = remove_master(records, "BetterBodies.esp")

    assert report.removed is True
    assert report.references_dropped == 0
    assert report.dropped_by_cell == {}
    assert report.remaining_masters == ["A.esm"]
    assert [(r.id, r.mast_index) for r in cell.references] == [("local", 0), ("from_a", 1)]


def test_remove_first_master_shifts_all_others() -> None:
    """Removing the first master shifts every later reference down one."""
    cell = _interior("C", [_ref("a", 1), _ref("b", 2), _ref("c", 3)])
    records = [_header("A.esm", "B.esm", "C.esm"), cell]

    report = remove_master(records, "A.esm")

    assert report.references_dropped == 1  # the ref into A
    assert [(r.id, r.mast_index) for r in cell.references] == [("b", 1), ("c", 2)]


def test_remove_is_case_insensitive() -> None:
    """The master name matches regardless of case, and the header casing is reported."""
    records = [_header("A.esm", "BetterBodies.esp")]

    report = remove_master(records, "betterbodies.ESP")

    assert report.removed is True
    assert report.master == "BetterBodies.esp"  # as it appeared in the header
    assert report.remaining_masters == ["A.esm"]


def test_remove_absent_master_is_noop() -> None:
    """A master not in the header leaves everything untouched."""
    cell = _interior("C", [_ref("a", 1)])
    records = [_header("A.esm"), cell]

    report = remove_master(records, "Nope.esm")

    assert report.removed is False
    assert report.remaining_masters == ["A.esm"]
    assert [(r.id, r.mast_index) for r in cell.references] == [("a", 1)]


def test_no_header_returns_not_removed() -> None:
    """With no TES3 header there is nothing to remove."""
    report = remove_master([Static(id="rock")], "A.esm")
    assert report.removed is False
    assert report.remaining_masters == []


def test_moved_reference_index_is_remapped() -> None:
    """A moved reference into a later master shifts down like any other."""
    moved = _ref("mover", 3, refr=5, moved_cell=(2, 2))
    records = [_header("A.esm", "B.esm", "C.esm"), _exterior((0, 0), [moved])]

    remove_master(records, "B.esm")

    assert moved.mast_index == 2
    assert moved.moved_cell == (2, 2)


def test_exterior_cell_uses_grid_label() -> None:
    """A dropped reference in an unnamed exterior is reported by its grid."""
    records = [_header("A.esm", "B.esm"), _exterior((3, -4), [_ref("gone", 2)])]

    report = remove_master(records, "B.esm")

    assert report.dropped_by_cell == {"(3, -4)": 1}


def test_round_trip_through_bytes() -> None:
    """remove_master_from_bytes renumbers references the writer then preserves."""
    cell = _interior("Hall", [_ref("from_a", 1), _ref("from_b", 2), _ref("from_c", 3)])
    data = write_plugin([_header("A.esm", "B.esm", "C.esm"), cell])

    new_data, report = remove_master_from_bytes(data, "B.esm")

    assert report.removed is True
    reparsed = read_plugin(new_data)
    header = next(r for r in reparsed if isinstance(r, Header))
    reparsed_cell = next(r for r in reparsed if isinstance(r, Cell))
    assert [name for name, _ in header.masters] == ["A.esm", "C.esm"]
    assert sorted((r.id, r.mast_index) for r in reparsed_cell.references) == [
        ("from_a", 1),
        ("from_c", 2),
    ]


def test_absent_master_bytes_round_trip_unchanged() -> None:
    """Removing a master that isn't there re-serialises the plugin faithfully."""
    cell = _interior("Hall", [_ref("from_a", 1)])
    data = write_plugin([_header("A.esm"), cell])

    new_data, report = remove_master_from_bytes(data, "Missing.esm")

    assert report.removed is False
    assert read_plugin(new_data)  # still a valid plugin


def test_rename_master_keeps_references_and_updates_size() -> None:
    """Renaming a master changes only the header entry; references are untouched."""
    cell = _interior("Hall", [_ref("a", 1), _ref("b", 2), _ref("c", 3)])
    header = _header("A.esm", "Old.esm", "C.esm")
    records = [header, cell]

    report = rename_master(records, "Old.esm", "New.esm", 4242)

    assert report.renamed is True
    assert report.remaining_masters == ["A.esm", "New.esm", "C.esm"]
    assert header.masters[1] == ("New.esm", 4242)
    assert [(r.id, r.mast_index) for r in cell.references] == [("a", 1), ("b", 2), ("c", 3)]


def test_rename_is_case_insensitive_on_the_old_name() -> None:
    """The old name matches regardless of case."""
    header = _header("A.esm", "BetterBodies.esp")
    report = rename_master([header], "betterbodies.ESP", "BB_Pluginless.esp", 9)
    assert report.renamed is True
    assert header.masters[1] == ("BB_Pluginless.esp", 9)


def test_rename_absent_master_is_noop() -> None:
    """A master not in the header leaves everything untouched."""
    header = _header("A.esm")
    report = rename_master([header], "Nope.esm", "New.esm", 1)
    assert report.renamed is False
    assert header.masters == [("A.esm", 1)]


def test_rename_onto_existing_master_raises() -> None:
    """Renaming onto another master would create a duplicate entry -- refused."""
    records = [_header("A.esm", "B.esm")]
    with pytest.raises(ValueError, match="already a master"):
        rename_master(records, "A.esm", "B.esm", 1)


def test_rename_onto_same_name_different_case_updates_in_place() -> None:
    """Renaming a master onto its own name (new case/size) is allowed."""
    header = _header("A.esm", "B.esm")
    report = rename_master([header], "b.esm", "B.ESM", 77)
    assert report.renamed is True
    assert header.masters[1] == ("B.ESM", 77)


def test_rename_no_header_returns_not_renamed() -> None:
    """With no TES3 header there is nothing to rename."""
    report = rename_master([Static(id="rock")], "A.esm", "B.esm", 1)
    assert report.renamed is False


def test_rename_round_trip_through_bytes() -> None:
    """rename_master_in_bytes rewrites the header the writer then preserves."""
    cell = _interior("Hall", [_ref("b", 2)])
    data = write_plugin([_header("A.esm", "Old.esm", "C.esm"), cell])

    new_data, report = rename_master_in_bytes(data, "Old.esm", "New.esm", 5150)

    assert report.renamed is True
    reparsed = read_plugin(new_data)
    header = next(r for r in reparsed if isinstance(r, Header))
    assert header.masters == [("A.esm", 1), ("New.esm", 5150), ("C.esm", 1)]
    reparsed_cell = next(r for r in reparsed if isinstance(r, Cell))
    assert [(r.id, r.mast_index) for r in reparsed_cell.references] == [("b", 2)]


@pytest.mark.parametrize("removed_pos_name", ["A.esm", "C.esm"])
def test_earlier_and_last_master_edges(removed_pos_name: str) -> None:
    """Removing the earliest or the last master keeps unrelated refs correct."""
    cell = _interior("C", [_ref("local", 0), _ref("mid", 2)])  # mid points at B
    records = [_header("A.esm", "B.esm", "C.esm"), cell]

    remove_master(records, removed_pos_name)

    # B was neither removed here; its ref stays valid (unshifted when A stays,
    # or shifted down to 1 when A is removed).
    expected = 1 if removed_pos_name == "A.esm" else 2
    assert next(r for r in cell.references if r.id == "mid").mast_index == expected
    assert next(r for r in cell.references if r.id == "local").mast_index == 0
