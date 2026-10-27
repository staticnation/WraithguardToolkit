"""The shared plan cases (``tests/fixtures/plan_cases.json``, which
``viewer-shell/ordercore/tests/plan_cases.rs`` runs through ``ordercore::plan`` too): a whole sort
through ``compute_plan`` as it is called (so through ordercore when the native module answers),
and the file itself against the generator, so a change in Python without regenerating them fails
here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "plan_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT))
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_plan_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_plan_cases


@pytest.mark.parametrize("case", DOC["cases"], ids=[c["name"] for c in DOC["cases"]])
def test_a_whole_sort_as_recorded(case: dict[str, Any]) -> None:
    got = _generator().run_case(case["name"], case["files"], case["argv"], case["extra"])
    assert got == case
