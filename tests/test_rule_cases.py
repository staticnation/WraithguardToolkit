"""The shared rule cases (``tests/fixtures/rule_cases.json``, which
``viewer-shell/ordercore/tests/rule_cases.rs`` runs through ``ordercore::rules`` too)
through :func:`~wraithguard.rules.parser.parse_mlox_text`, and the file itself against the
generator, so a rule changed in Python without regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

from wraithguard.rules.parser import parse_mlox_text

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "rule_cases.json"
CASES: list[dict[str, Any]] = json.loads(FILE.read_text(encoding="utf-8"))["cases"]


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_a_rule_text_gives_its_blocks(case: dict[str, Any]) -> None:
    blocks, skipped = parse_mlox_text(case["text"])
    assert [[k, names] for k, names in blocks] == case["blocks"]
    assert skipped == case["skipped"]


def test_the_case_file_is_what_the_generator_writes() -> None:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_rule_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    assert gen_rule_cases.cases() == CASES
