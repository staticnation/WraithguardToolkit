"""Building a patch: the shared build cases (``tests/fixtures/build_cases.json``, which
``viewer-shell/editcore/tests/cases.rs`` runs through ``editcore::build`` too) through
:func:`~wraithguard.patch.service.build_record_patch`, as the Patch Builder runs it.

A case is the source plugins' records (header included, file order), the load order, the
masters' sizes, the patch's file name and what it is made of; ``expect`` is the document
the build would write (header first) and what it reports, or the reason it is refused.
A dry run, and the document written as the service writes it (``bytes``, base64) - not
for one carrying a Lua script list, which the Python writer has no record for.
"""

from __future__ import annotations

import base64
import json
from pathlib import Path
from typing import Any

import pytest

from wraithguard.esp import plugin_from_json, write_plugin
from wraithguard.patch import service
from wraithguard.patch.merge import FieldChoice, FieldValue, Merge
from wraithguard.patch.records import NewRecord, Selection
from wraithguard.patch.refedit import NewRef, RefEdit

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "build_cases.json").read_text(encoding="utf-8")
)["cases"]


def plan(case: dict[str, Any]) -> dict[str, Any]:
    """A case's ``plan`` as the service takes it."""
    p = case["plan"]
    return {
        "selections": [
            Selection(plugin=a, record_type=b, key=c) for a, b, c in p.get("selections", [])
        ],
        "merges": [
            Merge(
                record_type=m["type"],
                key=m["key"],
                base_plugin=m["base"],
                choices=tuple(
                    (
                        FieldChoice(path=c["path"], plugin=c["plugin"])
                        if "plugin" in c
                        else FieldValue(path=c["path"], value=c["value"])
                    )
                    for c in m["choices"]
                ),
            )
            for m in p.get("merges", [])
        ],
        "carried": p.get("carried", []),
        "ref_edits": [
            RefEdit(
                cell=e["cell"],
                origin=e["origin"],
                refr_index=e["refr_index"],
                changes=e["changes"],
                plugins=tuple(e.get("plugins", [])),
            )
            for e in p.get("ref_edits", [])
        ],
        "new_refs": [
            NewRef(
                cell=n["cell"],
                uid=n["uid"],
                fields=n["fields"],
                base_plugin=n.get("base_plugin", ""),
                plugins=tuple(n.get("plugins", [])),
            )
            for n in p.get("new_refs", [])
        ],
        "new_records": [
            NewRecord(
                record_type=m["type"],
                key=m["key"],
                record=m["record"],
                source=m.get("source", ""),
                topic=m.get("topic", ""),
            )
            for m in p.get("new_records", [])
        ],
    }


def run(case: dict[str, Any], monkeypatch: pytest.MonkeyPatch) -> dict[str, Any]:
    """Build a case's patch (a dry run): the document and the result, or the refusal."""
    built: list[Any] = []
    real = service.build_plugin

    def keep(*a: Any, **kw: Any) -> Any:
        """The document, kept as the service makes it."""
        document = real(*a, **kw)
        built.append(json.loads(json.dumps(document)))
        return document

    monkeypatch.setattr(service, "build_plugin", keep)
    sources = json.loads(json.dumps(case["sources"]))
    try:
        result = service.build_record_patch(
            records_by_plugin=sources,
            load_order=case["order"],
            sizes=case["sizes"],
            converter="",
            output=Path(case["output"]),
            dry_run=True,
            **plan(case),
        )
    except service.PatchServiceError as exc:
        return {"error": str(exc)}
    assert sources == case["sources"], "a build changed the sources"
    out: dict[str, Any] = {"document": built[0]}
    if not any(r.get("type") == "ScriptConfigList" for r in built[0]):
        written = write_plugin(plugin_from_json([dict(r) for r in built[0]]))
        out["bytes"] = base64.b64encode(written).decode("ascii")
    return {
        **out,
        "records": result.records,
        "carried": result.carried,
        "masters": result.masters,
        "remapped": result.remapped,
        "lines": result.lines,
    }


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(monkeypatch: pytest.MonkeyPatch, case: dict[str, Any]) -> None:
    assert run(case, monkeypatch) == case["expect"]
