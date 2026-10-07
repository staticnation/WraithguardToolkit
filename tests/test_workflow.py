"""The Editor's workflow helpers (wraithguard.patch.workflow) and new cells and prefabs."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.patch import workflow as wf
from wraithguard.patch.editor import EditorError, EditorSession
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path


def _npc(rid: str, **kw: Any) -> dict[str, Any]:
    """An NPC record."""
    return {
        "type": "Npc",
        "id": rid,
        "name": rid.title(),
        "race": "",
        "class": "",
        "faction": "",
        "npc_flags": "",
        "data": {"rank": 0},
        **kw,
    }


PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        _npc("fargoth", race="Wood Elf", npc_flags="AUTO_CALCULATE"),
        _npc("sellus", race="Imperial", **{"class": "Trader"}),
        _npc("ajira", race="Khajiit", faction="Mages Guild", npc_flags="FEMALE", data={"rank": 3}),
        {"type": "MiscItem", "id": "gold_001", "name": "Gold"},
        {
            "type": "LeveledItem",
            "id": "l_inner",
            "chance_none": 0,
            "leveled_item_flags": "",
            "items": [["gold_001", 1]],
        },
        {
            "type": "LeveledItem",
            "id": "l_outer",
            "chance_none": 50,
            "leveled_item_flags": "CALCULATE_FROM_ALL_LEVELS",
            "items": [["l_inner", 1], ["gem", 5], ["junk", 10]],
        },
        {
            "type": "Script",
            "id": "TestScript",
            "text": "begin TestScript\nshort myVar\n; myVar in a comment\nset myVar to 1\nend\n",
        },
        {
            "type": "Cell",
            "name": "Vault",
            "data": {"grid": [0, 0], "flags": "IS_INTERIOR"},
            "references": [],
        },
    ],
}
_TAGS = {"NPC_": "Npc", "MISC": "MiscItem", "LEVI": "LeveledItem", "SCPT": "Script", "CELL": "Cell"}


def _session(tmp_path: Path) -> EditorSession:
    """A session over :data:`PLUGINS`."""
    return EditorSession(
        [(n, tmp_path / n) for n in PLUGINS],
        PatchQueue(),
        read=lambda p, tag: [r for r in PLUGINS[p.name] if r["type"] == _TAGS.get(tag)],
    )


def test_a_leveled_list_rolled():
    lst = {
        "chance_none": 0,
        "leveled_item_flags": "",
        "items": [["a", 1], ["b", 3], ["c", 3], ["d", 9]],
    }
    # Not from all levels: the highest of those at or below the level only.
    assert wf.roll_entries("LeveledItem", lst, 5) == {"b": 0.5, "c": 0.5}
    lst["leveled_item_flags"] = "CALCULATE_FROM_ALL_LEVELS"
    assert wf.roll_entries("LeveledItem", lst, 5) == pytest.approx(
        {"a": 1 / 3, "b": 1 / 3, "c": 1 / 3}
    )
    assert wf.roll_entries("LeveledItem", lst, 0) == {"": 1.0}


def test_lists_inside_lists(tmp_path):
    s = _session(tmp_path)
    outer = s.find("LEVI", "l_outer")
    assert outer is not None
    got = wf.leveled_roll(s, outer, 7)
    assert {o["id"]: o["chance"] for o in got["outcomes"]} == pytest.approx(
        {"": 0.5, "gold_001": 0.25, "gem": 0.25}
    )
    assert got["nested"] == ["l_inner"]


def test_who_can_say(tmp_path):
    s = _session(tmp_path)
    names = lambda info: [n["id"] for n in wf.who_can_say(s, info)["npcs"]]  # noqa: E731
    assert names({"speaker_race": "Wood Elf"}) == ["fargoth"]
    assert names({"speaker_faction": "Mages Guild", "data": {"speaker_rank": 4}}) == []
    assert names({"speaker_faction": "Mages Guild", "data": {"speaker_rank": 3}}) == ["ajira"]
    assert names({"data": {"speaker_sex": "Female"}}) == ["ajira"]
    got = wf.who_can_say(
        s,
        {
            "filters": [
                {"filter_type": "NotRace", "id": "Khajiit"},
                {"filter_type": "Journal", "id": "MS_Q"},
            ]
        },
    )
    assert [n["id"] for n in got["npcs"]] == ["fargoth", "sellus"]
    assert got["unchecked"] == ["Journal MS_Q"]


def test_filter_choices_name_what_records_hold():
    got = wf.filter_choices()
    assert "NotFaction" in got["types"] and "None" in got["types"]
    assert "PcLevel" in got["functions"] and got["comparisons"][0] == "Equal"


def test_scripts_naming_a_word(tmp_path):
    s = _session(tmp_path)
    got = wf.script_refs(s, "MYVAR")
    assert [(h["script"], h["line"]) for h in got["hits"]] == [("TestScript", 2), ("TestScript", 4)]
    words = wf.script_words(s)
    assert "AddItem" in words["functions"] and "begin" in [k.lower() for k in words["keywords"]]


def test_search_and_replace_everywhere(tmp_path):
    s = _session(tmp_path)
    found = wf.search_all(s, "elf")
    assert [(h["tag"], h["id"], h["path"]) for h in found["hits"]] == [("NPC_", "fargoth", "race")]
    out = wf.replace_all(s, found["hits"], "elf", "Mer")
    assert out == {"changed": 1, "failed": []}
    assert s.can_undo()["undo"] == 1
    assert wf.search_all(s, "Wood Mer")["count"] == 1
    # An id is not replaced here.
    bad = wf.replace_all(s, [{"tag": "NPC_", "id": "fargoth", "path": "id"}], "far", "near")
    assert bad["changed"] == 0 and "Search & Replace" in bad["failed"][0]["error"]


def test_new_cells_and_a_prefab_in_one(tmp_path):
    s = _session(tmp_path)
    assert s.new_cell("My Room") == {"cell": "My Room", "interior": True}
    assert s.new_cell("(12, -4)") == {"cell": "(12, -4)", "interior": False}
    with pytest.raises(EditorError, match="already"):
        s.new_cell("Vault")
    gold = s.find("MISC", "gold_001")
    assert gold is not None
    before = s.can_undo()["undo"]
    placed = s.place_many(
        "My Room", [(gold, [0, 0, 0], None, 1.0), (gold, [64, 0, 0], [0, 0, 1.5], 2.0)]
    )
    assert [p.cell for p in placed] == ["My Room", "My Room"]
    assert placed[1].fields["scale"] == 2.0 and s.can_undo()["undo"] == before + 1
    # In an exterior, each goes in the square its position is in.
    out = s.place_many("(12, -4)", [(gold, [12 * 8192 + 10, -4 * 8192 + 10, 0], None, 1.0)])
    assert out[0].cell == "(12, -4)"
