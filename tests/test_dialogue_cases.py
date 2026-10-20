"""The dialogue window and the dialogue's views: the shared dialogue cases
(``tests/fixtures/dialogue_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::dialogue`` and ``editcore::dialogue_graph`` too) through
:class:`~wraithguard.patch.editor.EditorSession`,
:class:`~wraithguard.patch.dialogue_graph.DialogueIndex` and
:func:`~wraithguard.patch.workflow.who_can_say`, composed as ``gui/editorlink.py`` composes
them."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.patch.dialogue_graph import DialogueIndex, npc_of
from wraithguard.patch.editor import TAG_TO_TYPE, EditorSession, queue_journal
from wraithguard.patch.merge import FieldValue
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.verify import as_written
from wraithguard.patch.workflow import who_can_say

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "dialogue_cases.json").read_text(encoding="utf-8")
)["cases"]


def _relabel(value: Any, labels: dict[str, str]) -> Any:
    """``value`` with every response id the patch made as its label."""
    if isinstance(value, str):
        return labels.get(value, value)
    if isinstance(value, list):
        return [_relabel(v, labels) for v in value]
    if isinstance(value, dict):
        return {k: _relabel(v, labels) for k, v in value.items()}
    return value


def dialogue_index(s: EditorSession) -> DialogueIndex:
    """The load order's dialogue with the pool's laid over (``_dialogue_index``)."""
    made = [
        (m.record_type, m.topic, m.key, dict(m.record))
        for m in s.queue.new_records
        if m.record_type in ("Dialogue", "DialogueInfo")
    ]
    edits: dict[str, dict[str, Any]] = {}
    for (record_type, key), choices in s.queue.fields.items():
        if record_type == "DialogueInfo":
            for c in choices:
                if isinstance(c, FieldValue):
                    edits.setdefault(key, {})[c.path] = c.value
    return DialogueIndex([(name, s._dialogue(name)) for name in s.order], made, edits)


def play(s: EditorSession, a: list[Any]) -> dict[str, Any]:
    """``editDialoguePlay`` ``{speaker, topic, choice, cell, disposition, strict}``."""
    speaker = a[0]
    if not speaker.strip():
        raise ValueError("Choose who to talk to (an NPC's id)")
    found = s.find("NPC_", speaker.strip(), None) or s.find("CREA", speaker.strip(), None)
    if found is None:
        raise ValueError(f"{speaker} is not an NPC or creature of this load order")
    npc = npc_of(found.record)
    out = dialogue_index(s).play(npc, a[1], choice=a[2], cell=a[3], disposition=a[4], strict=a[5])
    out["speaker"] = npc
    return out


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what each gave, and the journal after them."""
    plugins = case["plugins"]
    by_plugin = {p["name"]: p for p in plugins}
    given = {str(r.get("id")) for p in plugins for r in p["records"]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        p = by_plugin[path.name]
        if tag == "TES3":
            return [{"type": "Header", "masters": [[m, 0] for m in p.get("masters", [])]}]
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in p["records"] if r["type"] == want]

    def read_dialogue(path: Path) -> list[dict[str, Any]]:
        """One plugin's topics and responses, in file order."""
        records = by_plugin[path.name]["records"]
        return [
            json.loads(json.dumps(r)) for r in records if r["type"] in ("Dialogue", "DialogueInfo")
        ]

    s = EditorSession(
        [(p["name"], tmp / p["name"]) for p in plugins],
        PatchQueue(),
        read=read,
        read_dialogue=read_dialogue,
    )
    labels: dict[str, str] = {}
    ids: dict[str, str] = {}

    def rid(text: str) -> str:
        """The response a label (or an id) names."""
        return ids.get(text, text)

    results: list[Any] = []
    for op, *a in case["ops"]:
        try:
            if op == "topics":
                out: Any = s.topics()
            elif op == "topic":
                out = s.topic(a[0])
            elif op == "move_response":
                out = s.move_response(a[0], rid(a[1]), rid(a[2]))
            elif op == "new_response":
                out = s.view(s.new_response(a[0], rid(a[1])))
            elif op == "copy_topic":
                out = s.copy_topic(a[0], a[1])
            elif op == "set":
                found = s.find(a[0], rid(a[1]))
                assert found is not None, a
                s.set_field(found, a[2], a[3])
                out = "ok"
            elif op == "flow":
                out = dialogue_index(s).flow(a[0])
            elif op == "map":
                out = dialogue_index(s).neighbourhood(a[0], a[1])
            elif op == "flags":
                out = dialogue_index(s).flags(a[0], a[1])
            elif op == "play":
                out = play(s, a)
            elif op == "who_can_say":
                info = s.find("INFO", rid(a[0]))
                if info is None:
                    raise ValueError("That response is not in this load order")
                out = who_can_say(s, as_written(s, info))
            elif op == "pending":
                out = s.pending()
            elif op == "undo":
                out = s.undo()
            else:
                raise AssertionError(f"unknown operation {op}")
        except ValueError as exc:  # EditorError among them
            out = {"error": str(exc)}
        for made in s.queue.new_records:
            new = made.record_type == "DialogueInfo" and made.key not in given
            if new and made.key not in labels:
                labels[made.key] = f"R{len(labels) + 1}"
                ids[labels[made.key]] = made.key
        results.append(out)
    return _relabel({"results": results, "journal": queue_journal(s.queue)}, labels)


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
