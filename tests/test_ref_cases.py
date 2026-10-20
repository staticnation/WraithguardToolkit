"""Which version of a placed object stands: the shared reference cases
(``tests/fixtures/ref_cases.json``, which ``viewer-shell/ordercore`` runs too) through the
Editor's :func:`~wraithguard.patch.refedit.winning_reference`."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.patch.refedit import winning_reference

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "ref_cases.json").read_text(encoding="utf-8")
)["cases"]


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(case: dict[str, Any]) -> None:
    # One cell, each plugin's version of it holding that plugin's references.
    versions = [
        (
            p["name"],
            {
                "references": [
                    {"mast_index": m, "refr_index": i, "deleted": d, "id": "thing"}
                    for m, i, d in p["refs"]
                ]
            },
        )
        for p in case["plugins"]
    ]
    masters_of = {p["name"]: p["masters"] for p in case["plugins"]}
    origin, index = case["ask"]
    won = winning_reference(versions, masters_of, origin, index)
    want = case["expect"]
    if want["winner"] is None:
        assert won is None
        return
    assert won is not None
    assert (won[0], bool(won[1].get("deleted"))) == (want["winner"], want["deleted"])
