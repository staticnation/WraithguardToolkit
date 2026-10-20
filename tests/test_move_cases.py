"""Where a moved exterior reference stands: the shared move cases
(``tests/fixtures/move_cases.json``, which ``viewer-shell/viewcore/tests/ref_moves.rs`` runs
through the engine) through the Editor - :meth:`EditorSession.find_ref` in the home cell,
then the winning version's ``moved_cell``."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.patch.editor import EditorSession
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.records import HEADER_TYPE

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "move_cases.json").read_text(encoding="utf-8")
)["cases"]

HOME = (1, 1)


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    plugins = case["plugins"]
    by_plugin: dict[str, dict[str, list[dict[str, Any]]]] = {
        p["name"]: {
            "TES3": [{"type": HEADER_TYPE, "masters": [[m, 0] for m in p["masters"]]}],
            "CELL": [
                {
                    "type": "Cell",
                    "name": "",
                    "data": {"flags": 2, "grid": list(HOME)},
                    "references": [
                        {
                            "mast_index": m,
                            "refr_index": i,
                            "id": "rock",
                            "deleted": d,
                            "moved_cell": to,
                        }
                        for m, i, d, to in p["refs"]
                    ],
                }
            ],
        }
        for p in plugins
    }

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        return by_plugin[path.name].get(tag, [])

    s = EditorSession([(p["name"], tmp_path / p["name"]) for p in plugins], PatchQueue(), read=read)
    found = s.find_ref(f"({HOME[0]}, {HOME[1]})", "A.esm", 1)
    if found is None or found.ref.get("deleted"):
        got: list[list[int]] = []
    else:
        got = [list(found.ref.get("moved_cell") or HOME)]
    want = case["expect"]
    if isinstance(want, dict):  # an open case: each side's answer pinned apart
        want = want["editor"]
    assert got == want
