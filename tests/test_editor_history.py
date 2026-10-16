"""wraithguard.patch.editor: one undo history for every change the editor makes."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wraithguard.patch.editor import EditorSession
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path

PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {"type": "Weapon", "id": "iron sword", "name": "Iron Sword", "data": {"weight": 20.0}},
        {"type": "Cell", "name": "Vault", "data": {"grid": [0, 0], "flags": "IS_INTERIOR"}},
        {
            "type": "PathGrid",
            "flags": "",
            "cell": "Vault",
            "data": {"grid": [0, 0], "granularity": 1024, "point_count": 0},
            "points": [],
            "connections": [],
        },
    ],
}
_TAGS = {"WEAP": "Weapon", "CELL": "Cell", "PGRD": "PathGrid"}


def _session(tmp_path: Path) -> EditorSession:
    """A session over :data:`PLUGINS`, journalled."""
    return EditorSession(
        [(n, tmp_path / n) for n in PLUGINS],
        PatchQueue(),
        journal=tmp_path / "journal.json",
        read=lambda p, tag: [r for r in PLUGINS[p.name] if r["type"] == _TAGS.get(tag)],
    )


def test_one_history_for_fields_and_path_grids(tmp_path):
    s = _session(tmp_path)
    sword = s.find("WEAP", "iron sword")
    assert sword is not None
    assert s.can_undo() == {"undo": 0, "redo": 0}
    s.set_field(sword, "name", "Rusty Sword")
    s.set_pathgrid("int:Vault", [[0, 0, 0], [100, 0, 0]], [[0, 1], [1, 0]])
    assert s.can_undo() == {"undo": 2, "redo": 0}

    r = s.undo()  # the path grid goes
    assert r["done"] and any("Vault" in w for w in r["what"])
    assert s.pathgrid_view("int:Vault")["points"] == []
    assert ("Weapon", "iron sword") in s.queue.fields
    r = s.undo()  # then the name
    assert r["done"] and ("Weapon", "iron sword") not in s.queue.fields
    assert not s.undo()["done"]  # nothing more

    r = s.redo()
    assert r["done"] and s.queue.fields[("Weapon", "iron sword")][0].value == "Rusty Sword"
    assert s.can_undo() == {"undo": 1, "redo": 1}
    # A new change after an undo drops what could have been redone.
    s.set_field(sword, "data.weight", 5)
    assert s.can_undo()["redo"] == 0


def test_the_journal_follows_undo(tmp_path):
    s = _session(tmp_path)
    sword = s.find("WEAP", "iron sword")
    assert sword is not None
    s.set_field(sword, "name", "Rusty Sword")
    s.undo()
    # The pool is empty again: the journal goes (it is removed when empty).
    assert not (tmp_path / "journal.json").exists()
    s.redo()
    assert "Rusty Sword" in (tmp_path / "journal.json").read_text(encoding="utf-8")


def test_every_change_to_the_queue_moves_its_revision():
    q = PatchQueue()
    seen = [q.revision]
    q.set_base("Weapon", "a", "M.esm")
    seen.append(q.revision)
    q.remove_record("Weapon", "a")
    seen.append(q.revision)
    q.restore_snapshot(q.snapshot())
    seen.append(q.revision)
    q.clear()
    seen.append(q.revision)
    assert seen == sorted(set(seen))


def test_one_value_on_many_records_is_one_step(tmp_path):
    s = _session(tmp_path)
    sword = s.find("WEAP", "iron sword")
    assert sword is not None
    out = s.set_many([sword], "data.weight", 9)
    assert out == {"changed": 1, "failed": []}
    assert s.can_undo() == {"undo": 1, "redo": 0}
    bad = s.set_many([sword], "no.such.field", 1)
    assert bad["changed"] == 0 and bad["failed"][0]["id"] == "iron sword"


def test_a_batch_is_one_step(tmp_path):
    s = _session(tmp_path)
    sword = s.find("WEAP", "iron sword")
    assert sword is not None
    with s.batch():
        s.set_field(sword, "name", "A")
        s.set_field(sword, "data.weight", 3)
    assert s.can_undo()["undo"] == 1
    s.undo()
    assert ("Weapon", "iron sword") not in s.queue.fields


def test_a_batch_that_fails_leaves_nothing_behind(tmp_path):
    import pytest

    s = _session(tmp_path)
    sword = s.find("WEAP", "iron sword")
    assert sword is not None
    with pytest.raises(Exception), s.batch():  # noqa: B017
        s.set_field(sword, "name", "A")
        s.set_field(sword, "no.such.field", 1)
    assert ("Weapon", "iron sword") not in s.queue.fields
    assert len(s.queue) == 0
    assert s.can_undo()["undo"] == 0


def test_warm_reads_a_tag_once_so_a_later_lookup_reads_nothing(tmp_path):
    reads: list[tuple[str, str]] = []

    def read(path, tag):
        reads.append((path.name, tag))
        return [r for r in PLUGINS[path.name] if r["type"] == _TAGS.get(tag)]

    s = EditorSession(
        [(n, tmp_path / n) for n in PLUGINS], PatchQueue(), journal=tmp_path / "j.json", read=read
    )
    s.warm("CELL")
    n = len(reads)
    assert n == len(PLUGINS)
    s.find("CELL", "Vault")
    s.new_cell("(3, 4)")
    assert len(reads) == n  # nothing was read after the warm-up


def _counting_session(tmp_path):
    reads: list[tuple[str, str]] = []

    def read(path, tag):
        reads.append((path.name, tag))
        return [r for r in PLUGINS[path.name] if r["type"] == _TAGS.get(tag)]

    s = EditorSession(
        [(n, tmp_path / n) for n in PLUGINS], PatchQueue(), journal=tmp_path / "j.json", read=read
    )
    return s, reads


def test_a_cell_the_engine_found_absent_is_made_without_reading_the_load_order(tmp_path):
    s, reads = _counting_session(tmp_path)
    s.new_cell("(-28, 34)", absent=True)
    assert not [r for r in reads if r[1] == "CELL"]


def test_a_pending_cell_is_found_and_removed_without_reading_any_plugin(tmp_path):
    s, reads = _counting_session(tmp_path)
    s.new_cell("(-28, 34)", absent=True)
    found = s.find("CELL", "(-28, 34)")
    assert found is not None and found.new
    s.revert(found)
    assert s.queue.new_record("Cell", "(-28, 34)") is None
    assert not [r for r in reads if r[1] == "CELL"]
