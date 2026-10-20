"""The Editor's verifier: the shared verify cases (``tests/fixtures/verify_cases.json``, which
``viewer-shell/editcore/tests/cases.rs`` runs through ``editcore::verify`` too) through
:func:`~wraithguard.patch.verify.verify`, as ``gui/editorlink.py`` runs it.

A case's ``files`` are the data folders' files (the ones its records name and have); its
plugins are written to the test's folder for the script compiler (``write_plugins``, as
``cases.rs`` writes them): the records with an id, a script as the compiler makes it.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

import wraithguard_native

from wraithguard.esp.json import _by_name, record_to_json
from wraithguard.mwscript.compiler import ScriptCompiler
from wraithguard.patch.editor import TAG_TO_TYPE, EditorSession, queue_journal, tag_of
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.records import Selection
from wraithguard.patch.verify import verify

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "verify_cases.json").read_text(encoding="utf-8")
)["cases"]

#: Record types not written for the compiler (their packed fields are the crate's own).
UNWRITTEN = ("Landscape", "PathGrid", "Cell")


def write_plugins(case: dict[str, Any], folder: Path) -> None:
    """Each plugin of a case written for the compiler (``write_verify_plugins`` in cases.rs)."""
    extra: list[tuple[str, str, str, int, list[str], list[str], list[str]]] = []
    for p in case["plugins"]:
        for r in p["records"]:
            tag = tag_of(r["type"])
            if r.get("id") and tag:
                extra.append((tag, r["id"], str(r.get("script") or ""), -1, [], [], []))
    compiler = ScriptCompiler([], extra)
    for p in case["plugins"]:
        header = record_to_json(_by_name()["Header"]())
        header["masters"] = [[m, 0] for m in p.get("masters", [])]
        out: list[dict[str, Any]] = [header]
        for r in p["records"]:
            if r["type"] == "Script":
                made = compiler.compile_record(r["id"], r["text"], levels="weak")
                if made is not None:
                    out.append(json.loads(made))
            elif r["type"] not in UNWRITTEN:
                out.append(r)
        (folder / p["name"]).write_bytes(wraithguard_native.plugin_records_bytes(json.dumps(out)))


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what each gave, and the journal after them."""
    write_plugins(case, tmp)
    by_plugin = {p["name"]: p for p in case["plugins"]}
    files = {f.lower() for f in case.get("files", [])}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag, as the case gives them."""
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
        [(p["name"], tmp / p["name"]) for p in case["plugins"]],
        PatchQueue(),
        read=read,
        read_dialogue=read_dialogue,
    )

    def found(tag: str, rid: str) -> Any:
        """The record a case names."""
        record = s.find(tag, rid)
        assert record is not None, (tag, rid)
        return record

    results: list[Any] = []
    for op, *a in case["ops"]:
        try:
            if op == "set":
                s.set_field(found(a[0], a[1]), a[2], a[3])
                out: Any = "ok"
            elif op == "whole":
                s.queue.add_whole(Selection(plugin=a[2], record_type=TAG_TO_TYPE[a[0]], key=a[1]))
                out = "ok"
            elif op == "set_ref":
                ref = s.find_ref(a[0], a[1], a[2])
                assert ref is not None, a
                s.set_ref_field(ref, a[3], a[4])
                out = "ok"
            elif op == "insert":
                out = s.insert(a[0], a[1]).key
            elif op == "save_script":
                out = s.set_script_text(found(a[0], a[1]), a[2])
            elif op == "verify":
                out = verify(s, (lambda p: p.lower() in files) if a[0] else None)
            else:
                raise AssertionError(f"unknown operation {op}")
        except ValueError as exc:  # EditorError among them
            out = {"error": str(exc)}
        results.append(out)
    return {"results": results, "journal": queue_journal(s.queue)}


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
