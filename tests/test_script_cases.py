"""Script Edit and the rest of the Editor's reading: the shared script cases
(``tests/fixtures/script_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::mwscript`` and ``editcore::scripts`` too) through
:class:`~wraithguard.patch.editor.EditorSession`, :mod:`.patch.workflow`,
:mod:`wraithguard.mwscript` and :mod:`wraithguard.lua`, composed as ``gui/editorlink.py``
composes them.

The compiler reads plugin files, so each case's plugins are written to the test's folder
first (``write_plugins``, as ``cases.rs`` writes them): the records as given, a script as
the compiler makes it (the writer takes only the crate's own packed fields). The session
reads the records as given, so the listings come from the case's own bytecode.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

import wraithguard_native

from wraithguard.esp.json import _by_name, record_to_json
from wraithguard.lua.callgraph import call_graph_chart
from wraithguard.lua.flowchart import flowchart as lua_flowchart
from wraithguard.lua.parser import parse as lua_parse
from wraithguard.mwscript.compiler import ScriptCompiler
from wraithguard.mwscript.flowchart import flowchart
from wraithguard.mwscript.tes3conv import listing_for_bytecode_field
from wraithguard.patch.editor import TAG_TO_TYPE, EditorSession, queue_journal, tag_of
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.workflow import leveled_roll, script_refs, script_words

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "script_cases.json").read_text(encoding="utf-8")
)["cases"]

_KINDS = ("Topic", "Voice", "Greeting", "Persuasion", "Journal")


def write_plugins(case: dict[str, Any], folder: Path) -> None:
    """Write each plugin of a case for the compiler to read (``write_plugins`` in cases.rs).

    Every record with an id is laid into a first compiler, which then compiles each
    script (weakly: a name it cannot place is a warning) into the record the writer takes;
    one that does not compile is left out of the file.
    """
    extra: list[tuple[str, str, str, int, list[str], list[str], list[str]]] = []
    for p in case["plugins"]:
        for r in p["records"]:
            tag = tag_of(r["type"])
            if r.get("id") and tag:
                kind = str(r.get("dialogue_type") or "Topic")
                dial = (_KINDS.index(kind) if kind in _KINDS else 0) if tag == "DIAL" else -1
                extra.append((tag, r["id"], str(r.get("script") or ""), dial, [], [], []))
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
                continue
            out.append(r)
        (folder / p["name"]).write_bytes(wraithguard_native.plugin_records_bytes(json.dumps(out)))


def lua_chart(path: str, text: str, kind: str, line: int | None) -> dict[str, Any]:
    """``editLuaChart``, the chart given back rather than shown: ``{heading, chart, title}``."""
    tree = lua_parse(text, teal=path.lower().endswith(".tl"))
    kinds = {"Function", "LocalFunction", "FunctionStat"}
    by_line: dict[int, Any] = {}
    for n in tree.walk():
        if n.kind in kinds and n.line not in by_line:
            by_line[n.line] = n
    functions = [by_line[k] for k in sorted(by_line)]
    if kind == "functions":
        return {
            "functions": [
                {"name": n.value or "function at line " + str(n.line), "line": n.line}
                for n in functions
            ]
        }
    if kind == "calls":
        return {
            "heading": f"{path} - call graph",
            "chart": call_graph_chart(tree),
            "title": "Lua call graph",
        }
    target = next((n for n in functions if isinstance(line, int) and n.line == line), tree)
    name = target.value or ("function at line " + str(target.line))
    heading = path if target is tree else f"{path} - {name}"
    return {"heading": heading, "chart": lua_flowchart(target), "title": "Lua flowchart"}


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what each gave, and the journal after them."""
    write_plugins(case, tmp)
    by_plugin = {p["name"]: p for p in case["plugins"]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag, as the case gives them."""
        p = by_plugin[path.name]
        if tag == "TES3":
            return [{"type": "Header", "masters": [[m, 0] for m in p.get("masters", [])]}]
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in p["records"] if r["type"] == want]

    s = EditorSession(
        [(p["name"], tmp / p["name"]) for p in case["plugins"]], PatchQueue(), read=read
    )

    def found(tag: str, rid: str) -> Any:
        """The record a case names."""
        record = s.find(tag, rid)
        assert record is not None, (tag, rid)
        return record

    results: list[Any] = []
    for op, *a in case["ops"]:
        try:
            if op == "script":
                # editScript
                f = found(a[0], a[1])
                view = s.view(f)
                waiting = next(
                    (x.get("queued") for x in view["fields"] if x["path"] == "text"), None
                )
                source = (
                    a[2] if a[2] is not None else (waiting if isinstance(waiting, str) else None)
                )
                out: Any = s.script_view(f, source, s.script_compiler(s.pool_script_records()))
                out["queued"] = isinstance(waiting, str)
            elif op == "save_script":
                out = s.set_script_text(found(a[0], a[1]), a[2])
            elif op == "result":
                # editResult
                f = found("INFO", a[0])
                values = {
                    x["path"]: x.get("queued", x["value"])
                    for x in s.view(f)["fields"]
                    if x["path"] in ("script_text", "speaker_id")
                }
                source = a[1] if a[1] is not None else str(values.get("script_text") or "")
                out = s.result_check(
                    source,
                    str(values.get("speaker_id") or ""),
                    s.script_compiler(s.pool_script_records()),
                    stored=a[1] is None,
                )
            elif op == "words":
                out = script_words(s)
            elif op == "refs":
                out = script_refs(s, a[0])
            elif op == "flowchart":
                f = found(a[0], a[1])
                text = a[2] if isinstance(a[2], str) else str(f.record.get("text") or "")
                out = {
                    "heading": f"{f.key} - flowchart",
                    "chart": flowchart(text),
                    "title": "Script flowchart",
                }
            elif op == "roll":
                # editLeveledRoll
                level = a[2]
                if not isinstance(level, int) or isinstance(level, bool) or level < 1:
                    raise ValueError("level is a number from 1")
                f = s.find(a[0], a[1])
                if f is None or f.record_type not in ("LeveledItem", "LeveledCreature"):
                    raise ValueError("That is not a leveled list")
                out = leveled_roll(s, f, level)
            elif op == "lua":
                out = lua_chart(a[0], a[1], a[2], a[3])
            elif op == "listing":
                out = listing_for_bytecode_field(a[0], a[1])
            elif op == "insert":
                out = s.insert(a[0], a[1]).key
            elif op == "duplicate":
                out = s.duplicate(found(a[0], a[1]), a[2]).key
            elif op == "pending":
                out = s.pending()
            else:
                raise AssertionError(f"unknown operation {op}")
        except ValueError as exc:  # EditorError among them
            out = {"error": str(exc)}
        results.append(out)
    return {"results": results, "journal": queue_journal(s.queue)}


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
