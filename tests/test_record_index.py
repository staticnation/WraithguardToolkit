"""Which version of a record the Editor opens: the shared record cases
(``tests/fixtures/record_cases.json``, which ``viewer-shell/ordercore`` runs too) through
:meth:`EditorSession.find` - each type on its own, the last plugin winning, a deletion
taking the record out."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from wraithguard.patch.editor import TAG_TO_TYPE, EditorError, EditorSession
from wraithguard.patch.queue import PatchQueue

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "record_cases.json").read_text(encoding="utf-8")
)["cases"]


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path, case):
    names = [f"P{i}.esp" for i in range(len(case["plugins"]))]
    by_plugin = {
        name: [
            {"type": TAG_TO_TYPE[tag], "id": rid, "flags": "DELETED" if deleted else ""}
            for tag, rid, deleted in recs
        ]
        for name, recs in zip(names, case["plugins"], strict=True)
    }

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        return [r for r in by_plugin[path.name] if r["type"] == TAG_TO_TYPE[tag]]

    s = EditorSession([(n, tmp_path / n) for n in names], PatchQueue(), read=read)
    live = {(tag, rid): plugins for tag, rid, plugins in case["expect"]["live"]}
    every = {(tag, rid.lower()) for recs in case["plugins"] for tag, rid, _d in recs}
    for tag, rid in sorted(every):
        found = s.find(tag, rid)
        if (tag, rid) not in live:
            assert found is None, (tag, rid)
            continue
        assert found is not None, (tag, rid)
        assert [p for p, _r in found.versions] == [names[i] for i in live[tag, rid]], (tag, rid)


def test_a_new_object_may_not_take_another_types_id(tmp_path):
    recs = {"WEAP": [{"type": "Weapon", "id": "wg_clash", "flags": ""}]}

    def read(_path: Path, tag: str) -> list[dict[str, Any]]:
        """The one plugin's records of a tag."""
        return recs.get(tag, [])

    s = EditorSession([("A.esp", tmp_path / "A.esp")], PatchQueue(), read=read)
    with pytest.raises(EditorError, match="Weapon id"):
        s.insert("MISC", "WG_Clash")
    assert s.insert("SPEL", "wg_clash").tag == "SPEL"  # not an object: its own id space
