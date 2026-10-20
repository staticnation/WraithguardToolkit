"""The patch queue, its journal and undo: the shared queue cases
(``tests/fixtures/queue_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::queue`` too) through :class:`~wraithguard.patch.queue.PatchQueue`,
:func:`~wraithguard.patch.editor.queue_journal` and the session's undo history."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from wraithguard.patch.editor import EditorSession, queue_journal, restore_queue
from wraithguard.patch.merge import FieldChoice, FieldValue
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.records import NewRecord, Selection
from wraithguard.patch.refedit import NewRef, RefEdit

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "queue_cases.json").read_text(encoding="utf-8")
)["cases"]


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what the case checks after them."""
    q = PatchQueue()
    s = EditorSession([], q)
    snaps: dict[str, Any] = {}
    restored: list[int] = []
    stepped: list[bool] = []
    for op, *a in case["ops"]:
        if op == "whole":
            q.add_whole(Selection(plugin=a[2], record_type=a[0], key=a[1]))
        elif op == "field":
            c = a[3]
            choice = (
                FieldValue(a[2], c["value"]) if "value" in c else FieldChoice(a[2], c["plugin"])
            )
            q.add_field(a[0], a[1], choice)
        elif op == "base":
            q.set_base(a[0], a[1], a[2])
        elif op == "remove_record":
            q.remove_record(a[0], a[1])
        elif op == "remove_field":
            q.remove_field(a[0], a[1], a[2])
        elif op == "ref":
            q.add_ref_edit(RefEdit(a[0], a[1], a[2], dict(a[3]), tuple(a[4])))
        elif op == "remove_ref":
            q.remove_ref_edit(a[0], a[1], a[2], a[3])
        elif op == "made":
            q.add_new_record(NewRecord(a[0], a[1], dict(a[2]), a[3], a[4]))
        elif op == "remove_made":
            q.remove_new_record(a[0], a[1])
        elif op == "new":
            q.add_new_ref(NewRef(a[0], a[1], dict(a[2]), a[3], tuple(a[4]), a[5]))
        elif op == "remove_new":
            q.remove_new_ref(a[0], a[1])
        elif op == "snapshot":
            snaps[a[0]] = q.snapshot()
        elif op == "restore":
            q.restore_snapshot(snaps[a[0]])
        elif op == "consume":
            q.consume(snaps[a[0]])
        elif op == "clear":
            q.clear()
        elif op == "restore_journal":
            journal = tmp / "journal.json"
            journal.write_text(json.dumps(a[0]), encoding="utf-8")
            restored.append(restore_queue(q, journal))
        elif op == "step":
            s._record_step()
        elif op in ("undo", "redo"):
            stepped.append(bool((s.undo() if op == "undo" else s.redo())["done"]))
        else:
            raise AssertionError(f"unknown operation {op}")
    counts = s.can_undo()
    return {
        "journal": queue_journal(q),
        "len": len(q),
        "revision": q.revision,
        "restored": restored,
        "stepped": stepped,
        "history": [counts["undo"], counts["redo"]],
    }


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
