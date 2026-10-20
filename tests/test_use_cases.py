"""The Use Report and Search & Replace: the shared use cases
(``tests/fixtures/use_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::uses`` too) through :class:`~wraithguard.patch.editor.EditorSession`,
:func:`~wraithguard.patch.usesindex.index_rows` and
:func:`~wraithguard.mwscript.rename.replace_id`.

The script compiler is a stand-in on both sides (the real one moves into editcore with
Script Edit): a script compiles unless its text says ``nocompile``, and compiles to a
record whose fields the case can tell apart.
"""

from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.mwscript.compiler_data import CHAR_TYPES
from wraithguard.mwscript.rename import _is_symbol, replace_id
from wraithguard.patch.editor import TAG_TO_TYPE, EditorError, EditorSession, queue_journal
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.recordcache import RecordCache
from wraithguard.patch.refedit import RefEdit
from wraithguard.patch.usesindex import index_rows

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "use_cases.json").read_text(encoding="utf-8")
)["cases"]


class StubCompiler:
    """The cases' script compiler (``StubCompiler`` in ``editcore/tests/cases.rs`` too)."""

    def compile(self, text: str) -> SimpleNamespace:
        """Compiles unless the text says ``nocompile``."""
        if "nocompile" in text.lower():
            msg = SimpleNamespace(line=1, level="error", text="stub: does not compile")
            return SimpleNamespace(ok=False, messages=[msg])
        return SimpleNamespace(ok=True, messages=[])

    def compile_record(self, key: str, text: str) -> str:
        """A record the case can tell apart: the text's length, and the script's id."""
        return json.dumps(
            {
                "header": {
                    "num_shorts": 1,
                    "num_longs": 0,
                    "num_floats": 0,
                    "bytecode_length": len(text),
                    "variables_length": 2,
                },
                "variables": "eAA=",
                "bytecode": f"compiled:{key}",
            }
        )


def _relabel(value: Any, labels: dict[str, str]) -> Any:
    """``value`` with every new reference's editor name as its label."""
    if isinstance(value, str):
        return labels.get(value, value)
    if isinstance(value, list):
        return [_relabel(v, labels) for v in value]
    if isinstance(value, dict):
        return {k: _relabel(v, labels) for k, v in value.items()}
    return value


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what each gave, and the journal after them."""
    plugins = case["plugins"]
    by_plugin = {p["name"]: p for p in plugins}

    def header(p: dict[str, Any]) -> dict[str, Any]:
        """A plugin's header, from its masters."""
        return {"type": "Header", "masters": [[m, 0] for m in p.get("masters", [])]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        p = by_plugin[path.name]
        if tag == "TES3":
            return [header(p)]
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in p["records"] if r["type"] == want]

    def read_all(path: Path) -> list[dict[str, Any]]:
        """All of one plugin's records, in file order."""
        p = by_plugin[path.name]
        return [header(p), *(json.loads(json.dumps(r)) for r in p["records"])]

    s = EditorSession(
        [(p["name"], tmp / p["name"]) for p in plugins],
        PatchQueue(),
        read=read,
        read_all=read_all,
    )
    s.script_compiler = lambda extra=None: StubCompiler()  # type: ignore[assignment,method-assign,return-value]
    labels: dict[str, str] = {}

    def found(tag: str, rid: str) -> Any:
        """The record a case names."""
        record = s.find(tag, rid)
        assert record is not None, (tag, rid)
        return record

    results: list[Any] = []
    for op, *a in case["ops"]:
        try:
            if op == "uses":
                out: Any = s.uses(found(a[0], a[1]))
            elif op == "replace":
                # editReplace, as gui/editorlink.py composes it.
                plan, _report = s.replace_plan(found(a[0], a[1]), a[2], a[3])
                changed = s.replace_uses(plan)
                out = {
                    "changed": changed,
                    "refs": sum(1 for p in plan if not isinstance(p, tuple)),
                    "scripts": sum(
                        1
                        for p in plan
                        if isinstance(p, tuple)
                        and p[1] in ("text", "script_text")
                        and p[0].record_type in ("Script", "DialogueInfo")
                    ),
                }
            elif op == "plan":
                # What a plan holds, without queueing it.
                plan, _report = s.replace_plan(found(a[0], a[1]), a[2], a[3])
                out = [
                    (
                        {"ref": [p.cell, p.origin, p.refr_index, p.changes]}
                        if isinstance(p, RefEdit)
                        else (
                            {"new": [p.cell, p.uid, p.fields]}
                            if not isinstance(p, tuple)
                            else {"field": [p[0].tag, p[0].key, p[1], p[2]]}
                        )
                    )
                    for p in plan
                ]
            elif op == "script_text":
                out = s.set_script_text(found("SCPT", a[0]), a[1])
            elif op == "place":
                out = s.place(a[0], found(a[1], a[2]), a[3], None).uid
            elif op == "index":
                out = [list(row) for row in index_rows(read_all(tmp / a[0]))]
            elif op == "rename":
                out = list(replace_id(a[0], a[1], a[2]))
            elif op == "pending":
                out = s.pending()
            elif op == "can_undo":
                out = s.can_undo()
            elif op == "undo":
                out = s.undo()
            else:
                raise AssertionError(f"unknown operation {op}")
        except EditorError as exc:
            out = {"error": str(exc)}
        for n in s.queue.new_refs:
            labels.setdefault(n.uid, f"U{len(labels) + 1}")
        results.append(out)
    return _relabel({"results": results, "journal": queue_journal(s.queue)}, labels)


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_the_use_index_answers_as_reading_the_records_does(
    tmp_path: Path, case: dict[str, Any]
) -> None:
    """Each case's Use Report from the use index, as the Editor asks it, is the scan's.

    An id with a space (not one word) takes the index's strings and the scripts' and
    responses' words beside it: those uses come after the index's, so rows are compared
    without their order.
    """
    plugins = case["plugins"]
    by_plugin = {p["name"]: p for p in plugins}
    for p in plugins:
        (tmp_path / p["name"]).write_text(p["name"], encoding="utf-8")

    def header(p: dict[str, Any]) -> dict[str, Any]:
        """A plugin's header, from its masters."""
        return {"type": "Header", "masters": [[m, 0] for m in p.get("masters", [])]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        p = by_plugin[path.name]
        if tag == "TES3":
            return [header(p)]
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in p["records"] if r["type"] == want]

    def read_all(path: Path) -> list[dict[str, Any]]:
        """All of one plugin's records, in file order."""
        p = by_plugin[path.name]
        return [header(p), *(json.loads(json.dumps(r)) for r in p["records"])]

    order = [(p["name"], tmp_path / p["name"]) for p in plugins]
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        indexed = EditorSession(order, PatchQueue(), read=read, read_all=read_all, disk_cache=cache)
        assert indexed.index_uses(workers=1) == len(plugins)
        scanned = EditorSession(order, PatchQueue(), read=read, read_all=read_all)
        for op, *a in case["ops"]:
            if op != "uses":
                continue
            found = indexed.find(a[0], a[1])
            assert found is not None, a
            # The index answers (not the scan, as it would for a plugin not indexed).
            pairs = [indexed._paths[n.lower()] for n in indexed.order]
            assert indexed._uses_from_index(pairs, found) is not None, a
            want, got = (s.uses(found) for s in (scanned, indexed))
            assert got["uses"], a
            key = lambda u: (u["plugin"], u["type"], u["key"])  # noqa: E731
            assert sorted(got["uses"], key=key) == sorted(want["uses"], key=key), a
            assert (got["live"], got["cells"]) == (want["live"], want["cells"]), a


def test_an_id_character_is_an_ascii_word_character() -> None:
    """``editcore::rename::is_symbol``'s rule is the compiler table's symbol class."""
    for code in range(512):
        ch = chr(code)
        assert _is_symbol(ch) == (ch.isascii() and (ch.isalnum() or ch == "_")), code
    assert len(CHAR_TYPES) == 256
