"""wraithguard.patch.uses: the Use Report over a load order."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wraithguard.patch.editor import EditorSession
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.uses import use_report, uses_in

if TYPE_CHECKING:
    from pathlib import Path

PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {"type": "Header", "masters": []},
        {"type": "MiscItem", "id": "gold_001", "name": "Gold"},
        {"type": "LeveledItem", "id": "l_loot", "items": [["Gold_001", 1], ["x", 2]]},
        {"type": "Container", "id": "chest", "inventory": [[5, "gold_001"]]},
        {"type": "Script", "id": "payme", "text": "begin payme\\nplayer->additem gold_001 5\\nend"},
        {"type": "Script", "id": "other", "text": "set x to gold_0012"},  # not a word match
        {
            "type": "Cell",
            "id": "Vault",
            "name": "Vault",
            "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
            "references": [{"id": "gold_001"}, {"id": "GOLD_001"}, {"id": "chest"}],
        },
    ],
    "Fix.esp": [
        {"type": "Header", "masters": [["Morrowind.esm", 1]]},
        {"type": "LeveledItem", "id": "l_loot", "items": [["x", 2]]},  # took the gold out
    ],
}


def test_uses_in_finds_values_words_and_placements():
    uses = {
        (u.record_type, u.key): u
        for u in uses_in("Morrowind.esm", PLUGINS["Morrowind.esm"], "MiscItem", "GOLD_001")
    }
    assert set(uses) == {
        ("LeveledItem", "l_loot"),
        ("Container", "chest"),
        ("Script", "payme"),
        ("Cell", "Vault"),
    }
    assert uses[("LeveledItem", "l_loot")].paths == ["items.0.0"]
    assert uses[("Cell", "Vault")].count == 2 and uses[("Cell", "Vault")].paths == ["references"]
    assert uses[("Script", "payme")].paths == ["text"]


def test_use_report_marks_overridden_versions():
    order = [(name, name) for name in PLUGINS]
    uses = use_report(order, lambda p: PLUGINS[str(p)], "MiscItem", "gold_001")  # type: ignore[arg-type]
    loot = [u for u in uses if u.key == "l_loot"]
    assert [(u.plugin, u.wins) for u in loot] == [("Morrowind.esm", False)]  # Fix.esp took it out
    assert all(u.wins for u in uses if u.key != "l_loot")


def test_session_uses(tmp_path: Path):
    s = EditorSession(
        [(n, tmp_path / n) for n in PLUGINS],
        PatchQueue(),
        read=lambda p, tag: (
            [r for r in PLUGINS[p.name] if r["type"] == "MiscItem"] if tag == "MISC" else []
        ),
        read_all=lambda p: PLUGINS[p.name],
    )
    found = s.find("MISC", "gold_001")
    assert found is not None
    report = s.uses(found)
    assert report["cells"] == 2
    assert report["live"] == 4  # chest 1, script 1, vault 2; the leveled list's is overridden
    assert {u["tag"] for u in report["uses"]} == {"LEVI", "CONT", "SCPT", "CELL"}
