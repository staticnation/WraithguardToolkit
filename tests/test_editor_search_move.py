"""wraithguard.patch.editor: the Object Window's search by any field, and dialogue
responses moved by dragging."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.patch.editor import EditorError, EditorSession
from wraithguard.patch.merge import FieldValue
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path


def _info(i: str, prev: str, text: str) -> dict[str, Any]:
    """A response as the reader gives one."""
    return {
        "type": "DialogueInfo",
        "id": i,
        "prev_id": prev,
        "next_id": "",
        "text": text,
        "flags": "",
    }


PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {
            "type": "Weapon",
            "id": "iron sword",
            "name": "Iron Sword",
            "script": "",
            "data": {"weight": 20.0, "value": 30, "weapon_type": "LongBladeOneHand"},
        },
        {
            "type": "Weapon",
            "id": "daedric dagger",
            "name": "Daedric Dagger",
            "script": "daedricscript",
            "data": {"weight": 5.0, "value": 1500, "weapon_type": "ShortBladeOneHand"},
        },
        {
            "type": "Weapon",
            "id": "steel axe",
            "name": "Steel Axe",
            "script": "",
            "data": {"weight": 30.0, "value": 50, "weapon_type": "AxeTwoHand"},
        },
        {
            "type": "Container",
            "id": "chest",
            "name": "Chest",
            "inventory": [[5, "Gold_001"], [1, "iron sword"]],
        },
        {"type": "Dialogue", "id": "Rumors", "dialogue_type": "Topic"},
        _info("1", "", "First."),
        _info("2", "1", "Second."),
        _info("3", "2", "Third."),
    ],
}

_TAGS = {"WEAP": "Weapon", "CONT": "Container", "INFO": "DialogueInfo"}


def _session(tmp_path: Path) -> EditorSession:
    """A session over :data:`PLUGINS`."""
    return EditorSession(
        [(n, tmp_path / n) for n in PLUGINS],
        PatchQueue(),
        read=lambda p, tag: [r for r in PLUGINS[p.name] if r["type"] == _TAGS.get(tag)],
        read_dialogue=lambda p: PLUGINS[p.name],
    )


@pytest.mark.parametrize(
    ("query", "want"),
    [
        ("dagger", {"daedric dagger"}),  # any field
        ("script:daedric", {"daedric dagger"}),  # a field by path
        ("script:", {"daedric dagger"}),  # set at all
        ("data.weight>=20", {"iron sword", "steel axe"}),
        ("data.weight<10", {"daedric dagger"}),
        ("weapon_type=AxeTwoHand", {"steel axe"}),
        ("weapon_type!=AxeTwoHand data.value>40", {"daedric dagger"}),
        ('name:"iron sword"', {"iron sword"}),
        ("nothing-like-this", set()),
    ],
)
def test_search_any_field(tmp_path, query, want):
    got = _session(tmp_path).search("WEAP", query)
    assert set(got["hits"]) == want
    assert got["count"] == len(want)


def test_search_reads_lists_and_the_pool(tmp_path):
    s = _session(tmp_path)
    assert set(s.search("CONT", "gold_001")["hits"]) == {"chest"}
    hit = s.search("WEAP", "data.value>1000")["hits"]["daedric dagger"]
    assert hit == "data.value: 1500"
    # A typed value queued in the pool is what is searched.
    s.queue.add_field("Weapon", "iron sword", FieldValue(path="data.value", value=5000))
    assert set(s.search("WEAP", "data.value>1000")["hits"]) == {"daedric dagger", "iron sword"}
    with pytest.raises(EditorError):
        s.search("XXXX", "a")


def test_moving_a_response_is_its_prev_id(tmp_path):
    s = _session(tmp_path)
    t = s.move_response("rumors", "3", "")  # to the top
    assert [r["id"] for r in t["responses"]] == ["3", "1", "2"]
    q = {c.path: c.value for c in s.queue.fields[("DialogueInfo", "3")]}
    assert q == {"prev_id": "", "next_id": "1"}
    t = s.move_response("rumors", "1", "2")  # after the second
    assert [r["id"] for r in t["responses"]] == ["3", "2", "1"]
    # Already there: nothing more queued.
    before = dict(s.queue.fields)
    s.move_response("rumors", "1", "2")
    assert dict(s.queue.fields) == before
    with pytest.raises(EditorError):
        s.move_response("rumors", "1", "1")
    with pytest.raises(EditorError):
        s.move_response("rumors", "9", "")


def test_moving_the_patchs_own_response(tmp_path):
    s = _session(tmp_path)
    made = s.new_response("rumors", "3")
    t = s.move_response("rumors", made.key, "1")
    assert [r["id"] for r in t["responses"]] == ["1", made.key, "2", "3"]
    rec = s.queue.new_record("DialogueInfo", made.key)
    assert rec is not None and rec.record["prev_id"] == "1" and rec.record["next_id"] == "2"
