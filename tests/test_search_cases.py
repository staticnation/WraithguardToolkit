"""The Editor's searches and the Object Window's columns: the shared search cases
(``tests/fixtures/search_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::search`` and ``editcore::columns`` too) through
:class:`~wraithguard.patch.editor.EditorSession`, :mod:`.patch.columns` and
:mod:`.patch.workflow`."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.patch.armor import browser_table, settings_from
from wraithguard.patch.columns import table
from wraithguard.patch.editor import TAG_TO_TYPE, EditorError, EditorSession, queue_journal
from wraithguard.patch.merge import FieldChoice
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.records import Selection
from wraithguard.patch.workflow import filter_choices, replace_all, search_all

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "search_cases.json").read_text(encoding="utf-8")
)["cases"]


def columns(s: EditorSession, tag: str) -> dict[str, Any]:
    """``editColumns``, as ``gui/editorlink.py`` composes it."""
    tag = tag.strip().upper().ljust(4, "_")[:4]
    versions = [s._records(name, tag) for name in s.order]
    versions.append(s.column_overlay(tag))
    if tag != "ARMO":
        return table(tag, versions)
    settings = settings_from(s._records(name, "GMST") for name in s.order)
    out = table(tag, versions, settings)
    out["armorClass"] = browser_table(settings)
    return out


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what each gave, and the journal after them."""
    by_plugin = {p["name"]: p["records"] for p in case["plugins"]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in by_plugin[path.name] if r["type"] == want]

    s = EditorSession([(n, tmp / n) for n in by_plugin], PatchQueue(), read=read)
    results: list[Any] = []
    last_hits: list[dict[str, Any]] = []
    for op, *a in case["ops"]:
        try:
            if op == "set":
                found = s.find(a[0], a[1])
                assert found is not None, a
                s.set_field(found, a[2], a[3])
                out: Any = "ok"
            elif op == "choose":
                s.queue.add_field(TAG_TO_TYPE[a[0]], a[1], FieldChoice(a[2], a[3]))
                out = "ok"
            elif op == "whole":
                s.queue.add_whole(Selection(plugin=a[2], record_type=TAG_TO_TYPE[a[0]], key=a[1]))
                out = "ok"
            elif op == "insert":
                out = s.insert(a[0], a[1]).key
            elif op == "search":
                out = s.search(a[0], a[1], *a[2:])
            elif op == "columns":
                out = columns(s, a[0])
            elif op == "search_all":
                out = search_all(s, a[0], a[1], *a[2:])
                last_hits = out["hits"]
            elif op == "replace_all":
                hits = last_hits if a[0] == "last" else a[0]
                out = replace_all(s, hits, a[1], a[2], a[3])
            elif op == "filter_choices":
                out = filter_choices()
            elif op == "pending":
                out = s.pending()
            else:
                raise AssertionError(f"unknown operation {op}")
        except EditorError as exc:
            out = {"error": str(exc)}
        results.append(out)
    return {"results": results, "journal": queue_journal(s.queue)}


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
