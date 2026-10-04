"""wraithguard.patch.editor: the viewer editor's records, edits and journal."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.patch.editor import EditorError, EditorSession, coerce
from wraithguard.patch.merge import FieldChoice, FieldValue
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path

_PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {
            "type": "Npc",
            "id": "fargoth",
            "name": "Fargoth",
            "data": {"level": 2, "gold": 50},
            "flags": "",
        },
        {"type": "Static", "id": "ex_hut", "mesh": "x/hut.nif"},
        {
            "type": "Cell",
            "id": "Seyda Neen",
            "data": {"grid": [-2, -9], "flags": "IS_INTERIOR"},
            "references": [{}, {}],
        },
    ],
    "Fix.esp": [
        {
            "type": "Npc",
            "id": "Fargoth",
            "name": "Fargoth",
            "data": {"level": 3, "gold": 50},
            "flags": "",
        },
    ],
}


def _session(tmp_path: Path, reads: list | None = None) -> EditorSession:
    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        if reads is not None:
            reads.append((path.name, tag))
        want = {"NPC_": "Npc", "STAT": "Static", "CELL": "Cell"}[tag]
        return [r for r in _PLUGINS[path.name] if r["type"] == want]

    order = [(name, tmp_path / name) for name in _PLUGINS]
    return EditorSession(order, PatchQueue(), journal=tmp_path / "journal.json", read=read)


def test_find_reads_each_definer_once_in_load_order(tmp_path):
    reads: list = []
    s = _session(tmp_path, reads)
    found = s.find("npc_", "FARGOTH")
    assert found is not None
    assert found.key == "Fargoth"  # as the winning record spells it
    assert [p for p, _ in found.versions] == ["Morrowind.esm", "Fix.esp"]
    assert found.winner == "Fix.esp"
    s.find("NPC_", "fargoth", plugins=["Fix.esp", "Morrowind.esm"])
    assert sorted(reads) == [("Fix.esp", "NPC_"), ("Morrowind.esm", "NPC_")]
    assert s.find("NPC_", "nobody") is None
    assert s.find("XXXX", "fargoth") is None


def test_view_marks_identity_and_queued_values(tmp_path):
    s = _session(tmp_path)
    found = s.find("NPC_", "fargoth")
    s.queue.add_field("Npc", "Fargoth", FieldChoice(path="data.gold", plugin="Morrowind.esm"))
    view = s.view(found)
    assert view["winner"] == "Fix.esp" and view["plugins"] == ["Morrowind.esm", "Fix.esp"]
    fields = {f["path"]: f for f in view["fields"]}
    assert not fields["id"]["editable"] and not fields["type"]["editable"]
    assert fields["data.level"]["value"] == 3 and fields["data.level"]["editable"]
    assert fields["data.gold"]["queued"] == 50 and fields["data.gold"]["source"] == "Morrowind.esm"
    cell = s.view(s.find("CELL", "seyda neen"))
    refs = {f["path"]: f for f in cell["fields"]}["references"]
    assert refs["count"] == 2 and not refs["editable"]


def test_set_field_queues_a_typed_value_with_its_base(tmp_path):
    s = _session(tmp_path)
    found = s.find("NPC_", "fargoth")
    s.set_field(found, "data.level", "10")
    (merge,) = s.queue.merges(lambda _t, _k: "")  # the conflict scan does not list it
    assert merge.base_plugin == "Fix.esp"
    assert merge.choices == (FieldValue(path="data.level", value=10),)
    view = {f["path"]: f for f in s.view(found)["fields"]}
    assert view["data.level"]["queued"] == 10 and view["data.level"]["source"] == "typed"
    # Setting it back to what the record has drops the change.
    s.set_field(found, "data.level", 3)
    assert not len(s.queue)
    for bad in (("id", "x"), ("references", []), ("data.nope", 1)):
        with pytest.raises(EditorError):
            s.set_field(found, *bad)
    with pytest.raises(EditorError):
        s.set_field(found, "data.level", "lots")


def test_coerce():
    assert coerce(1, "2", None) == 2 and coerce(1, 2.0, None) == 2
    assert coerce(1.5, "2.25", None) == 2.25
    assert coerce(True, "false", None) is False
    assert coerce("a", 5, None) == "5"
    assert coerce([1], [2, 3], None) == [2, 3]
    with pytest.raises(EditorError):
        coerce(1, 2.5, None)
    with pytest.raises(EditorError):
        coerce(1, 300, "int:0:255")
    with pytest.raises(EditorError):
        coerce([1], "x", None)


def test_the_journal_survives_a_crash(tmp_path):
    s = _session(tmp_path)
    found = s.find("NPC_", "fargoth")
    s.set_field(found, "name", "Fargoth the Brave")
    s.set_field(s.find("STAT", "ex_hut"), "mesh", "x/hut2.nif")
    journal = json.loads((tmp_path / "journal.json").read_text(encoding="utf-8"))
    assert len(journal["fields"]) == 2
    # A new session (Wraithguard restarted) gets it all back, base included.
    again = _session(tmp_path)
    assert again.restore() == 2
    merges = {m.key: m for m in again.queue.merges(lambda _t, _k: "")}
    assert merges["Fargoth"].base_plugin == "Fix.esp"
    assert merges["ex_hut"].choices == (FieldValue(path="mesh", value="x/hut2.nif"),)
    # What the queue already holds wins over the journal.
    assert again.restore() == 0
    # Emptying the queue removes the journal.
    again.revert(again.find("NPC_", "fargoth"))
    again.revert(again.find("STAT", "ex_hut"), "mesh")
    assert not (tmp_path / "journal.json").exists()


def test_pending_lists_editor_and_conflict_choices(tmp_path):
    s = _session(tmp_path)
    s.set_field(s.find("NPC_", "fargoth"), "data.gold", 75)
    s.queue.add_field("Static", "ex_hut", FieldChoice(path="mesh", plugin="Morrowind.esm"))
    got = {p["id"]: p for p in s.pending()}
    assert got["Fargoth"]["tag"] == "NPC_"
    assert got["Fargoth"]["changes"] == [{"path": "data.gold", "value": 75}]
    assert got["ex_hut"]["changes"] == [{"path": "mesh", "plugin": "Morrowind.esm"}]


def test_plugins_from_cfg_takes_the_last_folder_that_has_each(tmp_path):
    from wraithguard.patch.editor import plugins_from_cfg

    a, b = tmp_path / "A", tmp_path / "B"
    a.mkdir()
    b.mkdir()
    (a / "Morrowind.esm").write_bytes(b"x")
    (a / "Mod.ESP").write_bytes(b"x")
    (b / "mod.esp").write_bytes(b"y")
    cfg = tmp_path / "openmw.cfg"
    cfg.write_text(
        f'data="{a}"\ndata="{b}"\ncontent=Morrowind.esm\ncontent=Mod.esp\n'
        "content=scripts.omwscripts\ncontent=Missing.esp\n",
        encoding="utf-8",
    )
    got = plugins_from_cfg(cfg)
    assert [(n, p.parent.name, p.name) for n, p in got] == [
        ("Morrowind.esm", "A", "Morrowind.esm"),
        ("Mod.esp", "B", "mod.esp"),
    ]


def test_coerce_checks_list_entries_against_the_first():
    inventory = [[5, "gold_001"], [1, "iron dagger"]]
    assert coerce(inventory, [["3", "gold_001"], [1, 2]], None) == [[3, "gold_001"], [1, "2"]]
    with pytest.raises(EditorError):
        coerce(inventory, [["x", "gold_001"]], None)
    packages = [{"type": "Wander", "distance": 128}]
    assert coerce(packages, [{"type": "Wander", "distance": "256"}], None) == [
        {"type": "Wander", "distance": 256}
    ]
    with pytest.raises(EditorError, match="not part"):
        coerce(packages, [{"type": "Wander", "speed": 1}], None)
    assert coerce([], ["anything"], None) == ["anything"]
    assert coerce(["fireball"], ["frost", 7], None) == ["frost", "7"]
