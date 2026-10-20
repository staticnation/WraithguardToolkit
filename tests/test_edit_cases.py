"""The record dialog and the changes made to records: the shared edit cases
(``tests/fixtures/edit_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::edit`` too) through :class:`~wraithguard.patch.editor.EditorSession`."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.patch.editor import TAG_TO_TYPE, EditorError, EditorSession, queue_journal
from wraithguard.patch.queue import PatchQueue

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "edit_cases.json").read_text(encoding="utf-8")
)["cases"]


#: A script's packed fields: zstd'd base64 where Python has a zstd backend, plain base64
#: where it has none (and the crate's own zstd frame may differ again), so the cases do
#: not hold them (``blank_records.json`` checks them, decoded).
PACKED = ("variables", "bytecode")


def _journal(doc: dict[str, Any]) -> dict[str, Any]:
    """The journal, a script's packed fields left out."""
    for made in doc.get("made") or []:
        for field in PACKED:
            if made["type"] == "Script" and field in made["record"]:
                made["record"][field] = "<packed>"
    return doc


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what each gave, and the journal after them."""
    by_plugin = {p["name"]: p["records"] for p in case["plugins"]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in by_plugin[path.name] if r["type"] == want]

    s = EditorSession([(n, tmp / n) for n in by_plugin], PatchQueue(), read=read)
    results: list[Any] = []
    for op, *a in case["ops"]:
        try:
            if op == "view":
                found = s.find(a[0], a[1])
                results.append(None if found is None else s.view(found))
            elif op == "set":
                found = s.find(a[0], a[1])
                if found is None:
                    results.append("not found")
                    continue
                s.set_field(found, a[2], a[3])
                results.append("ok")
            elif op == "set_many":
                founds = [f for f in (s.find(a[0], i) for i in a[1]) if f is not None]
                results.append(s.set_many(founds, a[2], a[3]))
            elif op == "revert":
                found = s.find(a[0], a[1])
                if found is None:
                    results.append("not found")
                    continue
                s.revert(found, a[2])
                results.append("ok")
            elif op == "insert":
                results.append(s.insert(a[0], a[1]).key)
            elif op == "duplicate":
                found = s.find(a[0], a[1])
                if found is None:
                    results.append("not found")
                    continue
                results.append(s.duplicate(found, a[2]).key)
            elif op == "pending":
                results.append(s.pending())
            elif op in ("undo", "redo"):
                results.append(s.undo() if op == "undo" else s.redo())
            else:
                raise AssertionError(f"unknown operation {op}")
        except EditorError as exc:
            results.append({"error": str(exc)})
    return {"results": results, "journal": _journal(queue_journal(s.queue))}


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
