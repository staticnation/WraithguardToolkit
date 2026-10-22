"""The shared pattern and version cases (``tests/fixtures/pattern_cases.json``, which
``viewer-shell/ordercore/tests/pattern_cases.rs`` runs through ``ordercore::patterns`` too)
through :func:`~wraithguard.sort.graph.expand_pattern` and
:func:`~wraithguard.versions.format_version`, and the file itself against the generator, so
a rule changed in Python without regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

from wraithguard.rules import pattern_has_meta
from wraithguard.sort.graph import expand_pattern
from wraithguard.versions import format_version

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "pattern_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))
POOL: list[str] = DOC["pool"]


@pytest.mark.parametrize("case", DOC["patterns"], ids=[c["name"] for c in DOC["patterns"]])
def test_a_pattern_stands_for_its_names(case: dict[str, Any]) -> None:
    assert pattern_has_meta(case["pattern"]) == case["has_meta"]
    assert expand_pattern(case["pattern"], POOL) == case["expand"]


@pytest.mark.parametrize("case", DOC["versions"], ids=[repr(c["in"]) for c in DOC["versions"]])
def test_a_version_is_made_comparable(case: dict[str, Any]) -> None:
    assert format_version(case["in"]) == case["out"]


def test_the_case_file_is_what_the_generator_writes() -> None:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_pattern_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    out = gen_pattern_cases.cases()
    assert out["patterns"] == DOC["patterns"]
    assert out["versions"] == DOC["versions"]
