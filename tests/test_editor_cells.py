"""wraithguard.patch.editor: Search & Replace in one cell, and whole cells copied."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.patch.editor import EditorError, EditorSession
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.refedit import NewRef, RefEdit, place_new_refs

if TYPE_CHECKING:
    from pathlib import Path


def _ref(mast: int, refr: int, rid: str, x: float, **more: Any) -> dict[str, Any]:
    """A placed reference as the reader gives one."""
    return {
        "mast_index": mast,
        "refr_index": refr,
        "id": rid,
        "temporary": True,
        "translation": [x, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
        **more,
    }


def _cell(name: str, *refs: dict[str, Any]) -> dict[str, Any]:
    """An interior's CELL record."""
    return {
        "type": "Cell",
        "flags": "",
        "name": name,
        "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
        "references": list(refs),
    }


PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {"type": "Header", "masters": []},
        {"type": "MiscItem", "id": "gold_001", "name": "Gold"},
        {"type": "MiscItem", "id": "gold_005", "name": "Gold"},
        _cell("Vault", _ref(0, 1, "gold_001", 1.0), _ref(0, 2, "chest", 2.0)),
        _cell("Hall", _ref(0, 3, "gold_001", 5.0)),
        {
            "type": "PathGrid",
            "flags": "",
            "cell": "Vault",
            "data": {"grid": [0, 0], "granularity": 1024, "point_count": 1},
            "points": [{"location": [1, 2, 3], "auto_generated": 1, "connection_count": 0}],
            "connections": [],
        },
    ],
    "Fix.esp": [
        {"type": "Header", "masters": [["Morrowind.esm", 1]]},
        # Fix moves the chest and deletes nothing; adds a lamp of its own.
        _cell("Vault", _ref(1, 2, "chest", 9.0), _ref(0, 1, "lamp", 4.0)),
    ],
}

_TAGS = {"TES3": "Header", "CELL": "Cell", "MISC": "MiscItem", "PGRD": "PathGrid"}


def _session(tmp_path: Path) -> EditorSession:
    """A session over :data:`PLUGINS`."""
    order = [(n, tmp_path / n) for n in PLUGINS]
    return EditorSession(
        order,
        PatchQueue(),
        journal=tmp_path / "journal.json",
        read=lambda p, tag: [r for r in PLUGINS[p.name] if r["type"] == _TAGS.get(tag)],
        read_all=lambda p: PLUGINS[p.name],
    )


def test_replace_in_one_cell_only(tmp_path):
    s = _session(tmp_path)
    gold = s.find("MISC", "gold_001")
    assert gold is not None
    plan, _report = s.replace_plan(gold, "gold_005", cell="hall")
    assert all(isinstance(p, RefEdit) for p in plan)
    assert [(p.cell, p.refr_index) for p in plan if isinstance(p, RefEdit)] == [("Hall", 3)]
    everywhere, _ = s.replace_plan(gold, "gold_005")
    assert len([p for p in everywhere if isinstance(p, RefEdit)]) == 2


def test_replace_reaches_the_patchs_own_new_references(tmp_path):
    s = _session(tmp_path)
    gold = s.find("MISC", "gold_001")
    assert gold is not None
    s.queue.add_new_ref(NewRef("Hall", "new-1", {"id": "gold_001", "translation": [0, 0, 0]}))
    s.queue.add_new_ref(NewRef("Vault", "new-2", {"id": "gold_001", "translation": [0, 0, 0]}))
    plan, _ = s.replace_plan(gold, "gold_005", cell="Hall")
    assert s.replace_uses(plan) == 2
    ids = {n.uid: n.fields["id"] for n in s.queue.new_refs}
    assert ids == {"new-1": "gold_005", "new-2": "gold_001"}


def test_a_cell_copied_whole(tmp_path):
    s = _session(tmp_path)
    # A change queued in the pool goes with the copy.
    s.queue.add_ref_edit(RefEdit("Vault", "Morrowind.esm", 1, {"translation": [7.0, 0, 0]}))
    out = s.duplicate_cell("int:Vault", "Vault Copy")
    assert out == {"cell": "Vault Copy", "refs": 3, "pathgrid": True}
    made = s.queue.new_record("Cell", "Vault Copy")
    assert made is not None and made.record["name"] == "Vault Copy"
    assert "references" not in made.record
    placed = {n.fields["id"]: n for n in s.queue.new_refs if n.cell == "Vault Copy"}
    assert set(placed) == {"gold_001", "chest", "lamp"}
    assert placed["chest"].fields["translation"][0] == 9.0  # Fix's version wins
    assert placed["gold_001"].fields["translation"][0] == 7.0  # the pool's change
    assert placed["lamp"].base_plugin == "Fix.esp"
    grid = s.queue.new_record("PathGrid", "Vault Copy")
    assert grid is not None and grid.record["cell"] == "Vault Copy"
    # And it writes: each reference becomes the patch's own (0, n) in the new cell.
    records = [dict(made.record)]
    given = place_new_refs(records, list(s.queue.new_refs), {}, [])
    assert len(given) == 3
    assert sorted(r["refr_index"] for r in records[0]["references"]) == [1, 2, 3]
    assert all(r["mast_index"] == 0 for r in records[0]["references"])


@pytest.mark.parametrize(
    ("cell", "name", "why"),
    [
        ("3,-1", "X", "only an interior"),
        ("(3, -1)", "X", "only an interior"),
        ("Vault", "", "needs a name"),
        ("Vault", "Hall", "already"),
        ("Nowhere", "X", "no plugin"),
        ("Vault", "x" * 64, "at most"),
    ],
)
def test_bad_copies_are_refused(tmp_path, cell, name, why):
    with pytest.raises(EditorError, match=why):
        _session(tmp_path).duplicate_cell(cell, name)
