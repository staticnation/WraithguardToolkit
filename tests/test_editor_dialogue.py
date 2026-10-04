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
        {"type": "Dialogue", "id": "MS_Quest", "dialogue_type": "Journal"},
        _info("j0", "", "The Quest", quest_state="Name", data={"disposition": 0}),
        _info("j10", "j0", "Begun.", data={"disposition": 10}),
        _info("j100", "j10", "Done.", quest_state="Finished", data={"disposition": 100}),
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
        ("Journal", "MS_Quest", ["Morrowind.esm"]),
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


def test_a_new_response_goes_where_it_was_put(tmp_path):
    s = _session(tmp_path)
    made = s.new_response("rumors", "5")  # after "Inserted.", before "Second, changed."
    rec = made.record
    assert made.new and rec["prev_id"] == "5" and rec["next_id"] == "2"
    assert rec["data"]["dialogue_type"] == "Topic"
    s.set_field(s.find("INFO", made.key), "text", "Brand new.")  # type: ignore[arg-type]
    order = [r["id"] for r in s.topic("Rumors")["responses"]]
    assert order == ["1", "5", made.key, "2", "7"]
    assert s.topic("Rumors")["responses"][2]["text"] == "Brand new."
    top = s.new_response("Rumors")
    assert top.record["prev_id"] == "" and top.record["next_id"] == "1"
    assert s.topic("Rumors")["responses"][0]["id"] == top.key
    queued = s.queue.new_record("DialogueInfo", made.key)
    assert queued is not None and queued.topic == "Rumors" and queued.source == "Mod.esp"
    with pytest.raises(EditorError, match="not a response"):
        s.new_response("Rumors", "nope")


def test_a_new_response_is_written_inside_its_topic(tmp_path):
    from wraithguard.patch.records import NewRecord
    from wraithguard.patch.service import build_record_patch

    plugins = {"Mod.esp": [{"type": "Header", "masters": []}, *PLUGINS["Mod.esp"]]}
    made = NewRecord(
        "DialogueInfo",
        "999",
        {"type": "DialogueInfo", "id": "999", "prev_id": "5", "text": "New."},
        "Mod.esp",
        "Rumors",
    )
    captured: dict[str, Any] = {}
    from wraithguard.patch import service

    real = service._write
    service._write = lambda doc, _t, _c: captured.setdefault("doc", doc)  # type: ignore[assignment]
    try:
        build_record_patch(
            [], plugins, ["Mod.esp"], {"Mod.esp": 1}, "", tmp_path / "p.esp", new_records=[made]
        )
    except FileNotFoundError:
        pass  # the stand-in writer wrote nothing to measure
    finally:
        service._write = real  # type: ignore[assignment]
    kinds = [
        (r["type"], r.get("id"))
        for r in captured["doc"]
        if r["type"] in ("Dialogue", "DialogueInfo")
    ]
    assert kinds == [("Dialogue", "rumors"), ("DialogueInfo", "999")]


def test_a_journal_shows_its_stages(tmp_path):
    rows = _session(tmp_path).topic("ms_quest")["responses"]
    assert [(r["disposition"], r["quest"]) for r in rows] == [
        (0, "Name"),
        (10, ""),
        (100, "Finished"),
    ]
