"""A response merged field by field is written inside its topic, as a whole one is."""

from __future__ import annotations

from typing import Any

from wraithguard.patch.merge import FieldValue, Merge
from wraithguard.patch.records import Selection

PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {"type": "Header", "masters": []},
        {"type": "Dialogue", "id": "Greeting 0", "dialogue_type": "Greeting", "flags": ""},
        {
            "type": "DialogueInfo",
            "id": "1001",
            "prev_id": "",
            "next_id": "1002",
            "text": "Hello.",
            "flags": "",
        },
        {
            "type": "DialogueInfo",
            "id": "1002",
            "prev_id": "1001",
            "next_id": "",
            "text": "Hi.",
            "flags": "",
        },
        {"type": "Dialogue", "id": "Rumors", "dialogue_type": "Topic", "flags": ""},
        {
            "type": "DialogueInfo",
            "id": "2001",
            "prev_id": "",
            "next_id": "",
            "text": "Rumor.",
            "flags": "",
        },
    ],
}
ORDER = ["Morrowind.esm"]


def _build(selections: list[Selection], merges: list[Merge]) -> list[dict[str, Any]]:
    from wraithguard.patch.service import build_record_patch

    captured: dict[str, Any] = {}

    def fake_write(document: Any, target: Any, converter: str) -> None:
        captured["doc"] = document

    from wraithguard.patch import service

    real = service._write
    service._write = fake_write  # type: ignore[assignment]
    try:
        build_record_patch(
            selections,
            PLUGINS,
            ORDER,
            {"Morrowind.esm": 100},
            "",
            __import__("pathlib").Path("x.esp"),
            merges=merges,
            dry_run=False,
        )
    except FileNotFoundError:
        pass
    finally:
        service._write = real  # type: ignore[assignment]
    return [r for r in captured["doc"] if r.get("type") in ("Dialogue", "DialogueInfo")]


def _merge(key: str, text: str) -> Merge:
    return Merge("DialogueInfo", key, "Morrowind.esm", (FieldValue("text", text),))


def test_a_merged_response_comes_after_its_topic():
    recs = _build([], [_merge("1002", "Well met.")])
    assert [(r["type"], r["id"]) for r in recs] == [
        ("Dialogue", "Greeting 0"),
        ("DialogueInfo", "1002"),
    ]
    assert recs[1]["text"] == "Well met."


def test_responses_of_two_topics_each_follow_their_own():
    recs = _build(
        [Selection("Morrowind.esm", "DialogueInfo", "1001")],
        [_merge("2001", "Rumor two."), _merge("1002", "Well met.")],
    )
    order = [(r["type"], r["id"]) for r in recs]
    assert order == [
        ("Dialogue", "Greeting 0"),
        ("DialogueInfo", "1001"),
        ("DialogueInfo", "1002"),
        ("Dialogue", "Rumors"),
        ("DialogueInfo", "2001"),
    ]
