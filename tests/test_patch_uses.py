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


def test_search_and_replace_repoints_live_uses(tmp_path: Path):
    plugins = {
        "Morrowind.esm": [
            {"type": "Header", "masters": []},
            {"type": "MiscItem", "id": "gold_001", "name": "Gold"},
            {"type": "MiscItem", "id": "gold_005", "name": "Gold"},
            {"type": "LeveledItem", "id": "l_loot", "items": [["Gold_001", 1], ["x", 2]]},
            {"type": "LeveledItem", "id": "l_old", "items": [["gold_001", 1]]},
            {"type": "Container", "id": "chest", "inventory": [[5, "gold_001"]], "flags": ""},
            {"type": "Script", "id": "payme", "text": "player->additem gold_001 5"},
            {
                "type": "Cell",
                "id": "Vault",
                "name": "Vault",
                "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
                "references": [
                    {"mast_index": 0, "refr_index": 1, "id": "gold_001"},
                    {"mast_index": 0, "refr_index": 2, "id": "gold_001", "deleted": True},
                ],
            },
        ],
        "Fix.esp": [
            {"type": "Header", "masters": [["Morrowind.esm", 1]]},
            {"type": "LeveledItem", "id": "l_old", "items": []},  # overrides: not live
        ],
    }
    tags = {
        "MISC": "MiscItem",
        "LEVI": "LeveledItem",
        "CONT": "Container",
        "CELL": "Cell",
        "TES3": "Header",
    }
    s = EditorSession(
        [(n, tmp_path / n) for n in plugins],
        PatchQueue(),
        read=lambda p, tag: [r for r in plugins[p.name] if r["type"] == tags.get(tag)],
        read_all=lambda p: plugins[p.name],
    )
    found = s.find("MISC", "gold_001")
    assert found is not None
    import pytest

    from wraithguard.patch.editor import EditorError

    with pytest.raises(EditorError, match="not a MiscItem"):
        s.replace_plan(found, "nothing")
    plan, report = s.replace_plan(found, "GOLD_005")
    fields = [p for p in plan if isinstance(p, tuple)]
    refs = [p for p in plan if not isinstance(p, tuple)]
    assert {(u.key, f) for u, f, _v in fields} == {("l_loot", "items"), ("chest", "inventory")}
    assert report["cells"] == 2  # the deleted one is a use in the text, not replaced
    assert [(r.cell, r.origin, r.refr_index, dict(r.changes)) for r in refs] == [
        ("Vault", "Morrowind.esm", 1, {"id": "gold_005"})
    ]
    assert s.replace_uses(plan) == 3
    fields = {k: v[0].value for k, v in s.queue.fields.items()}
    assert fields[("LeveledItem", "l_loot")] == [["gold_005", 1], ["x", 2]]
    assert fields[("Container", "chest")] == [[5, "gold_005"]]
    assert s.queue.base("Container", "chest") == "Morrowind.esm"


def test_the_native_scan_agrees_with_the_python_walk(tmp_path):
    """Real plugin files: the Rust Use Report and the Python one give the same uses."""
    import json

    import pytest

    native = pytest.importorskip("wraithguard_native")
    if not hasattr(native, "use_report"):
        pytest.skip("the backend has no use_report")
    from wraithguard.esp.json import record_to_json
    from wraithguard.esp.records.cell import Cell
    from wraithguard.esp.records.container import Container
    from wraithguard.esp.records.leveleditem import LeveledItem
    from wraithguard.esp.records.miscitem import MiscItem
    from wraithguard.esp.records.script import Script
    from wraithguard.land.emit import build_plugin
    from wraithguard.patch.uses import native_use_report

    def rec(cls: Any, **fields: Any) -> dict[str, Any]:
        return {**record_to_json(cls()), **fields}

    ref = {
        "mast_index": 0,
        "refr_index": 1,
        "id": "gold_001",
        "temporary": True,
        "translation": [0.0, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
    }
    base = [
        rec(MiscItem, id="gold_001", name="Gold"),
        rec(LeveledItem, id="l_loot", items=[["Gold_001", 1], ["other", 2]]),
        rec(Container, id="chest", name="Chest", inventory=[[5, "gold_001"]]),
        rec(Script, id="payme", text="begin payme\nplayer->additem gold_001 5\nend"),
        rec(Cell, name="Vault", data={"flags": "IS_INTERIOR", "grid": [0, 0]}, references=[ref]),
    ]
    fix = [rec(LeveledItem, id="l_loot", items=[["other", 2]])]
    paths = []
    for name, recs, masters in (
        ("Base.esm", base, [("Morrowind.esm", 1)]),
        ("Fix.esp", fix, [("Morrowind.esm", 1), ("Base.esm", 1000)]),
    ):
        doc = build_plugin(recs, masters, description="t")
        p = tmp_path / name
        p.write_bytes(native.plugin_records_bytes(json.dumps(doc)))
        paths.append((name, p))
    other = tmp_path / "Other.esp"  # holds no "gold_001" at all: never parsed
    empty = build_plugin([], [("Morrowind.esm", 1)], description="t")
    other.write_bytes(native.plugin_records_bytes(json.dumps(empty)))
    paths.append(("Other.esp", other))

    def key(u):
        return (u.plugin, u.record_type, u.key, tuple(u.paths), u.count, u.wins)

    fast = native_use_report(paths, "MiscItem", "GOLD_001")
    slow = use_report(
        paths, lambda p: native.plugin_records(p.read_bytes()), "MiscItem", "GOLD_001"
    )
    assert fast is not None
    assert sorted(map(key, fast)) == sorted(map(key, slow))
    got = {(u.record_type, u.key): u for u in fast}
    assert got[("LeveledItem", "l_loot")].wins is False  # Fix.esp took it out
    assert got[("Cell", "Vault")].count == 1 and got[("Script", "payme")].paths == ["text"]
