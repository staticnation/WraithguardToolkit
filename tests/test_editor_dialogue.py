"""The editor's dialogue window: topics, and a topic's responses as the engine orders them."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.patch.editor import EditorError, EditorSession
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path


def _info(i: str, prev: str, text: str, **more: Any) -> dict[str, Any]:
    return {"type": "DialogueInfo", "id": i, "prev_id": prev, "text": text, "flags": "", **more}


PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {"type": "Dialogue", "id": "Rumors", "dialogue_type": "Topic"},
        _info("1", "", "First.", speaker_race="Dark Elf"),
        _info("2", "1", "Second.", data={"disposition": 30}),
        {"type": "Dialogue", "id": "Greeting 0", "dialogue_type": "Greeting"},
        _info("9", "", "Hello."),
    ],
    "Mod.esp": [
        {"type": "Dialogue", "id": "rumors", "dialogue_type": "Topic"},
        _info("5", "1", "Inserted.", speaker_id="fargoth", filters=[{"x": 1}]),
        _info("2", "1", "Second, changed.", data={"disposition": 40}),
        _info("7", "gone", "Orphaned."),
    ],
}


def _session(tmp_path: Path) -> EditorSession:
    return EditorSession(
        [(n, tmp_path / n) for n in PLUGINS],
        PatchQueue(),
        read=lambda _p, _t: [],
        read_dialogue=lambda p: PLUGINS[p.name],
    )


def test_topics_by_type_with_their_plugins(tmp_path):
    topics = _session(tmp_path).topics()
    assert [(t["type"], t["id"], t["plugins"]) for t in topics] == [
        ("Greeting", "Greeting 0", ["Morrowind.esm"]),
        ("Topic", "Rumors", ["Morrowind.esm", "Mod.esp"]),
    ]


def test_a_topic_in_engine_order(tmp_path):
    t = _session(tmp_path).topic("RUMORS")
    assert t["type"] == "Topic"
    rows = {r["id"]: r for r in t["responses"]}
    # Mod.esp inserts 5 straight after 1; 2 keeps its predecessor; 7 names none, so last.
    assert [r["id"] for r in t["responses"]] == ["1", "5", "2", "7"]
    assert rows["2"]["text"] == "Second, changed." and rows["2"]["winner"] == "Mod.esp"
    assert rows["2"]["plugins"] == ["Morrowind.esm", "Mod.esp"] and rows["2"]["disposition"] == 40
    assert rows["5"]["speaker"] == "who: fargoth, 1 condition(s)"
    assert rows["1"]["speaker"] == "race: Dark Elf"
    assert rows["7"]["orphan"] and not rows["1"]["orphan"]
    with pytest.raises(EditorError, match="no plugin"):
        _session(tmp_path).topic("nothing")
