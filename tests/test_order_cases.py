"""The shared sort-order cases (``tests/fixtures/order_cases.json``, which
``viewer-shell/ordercore/tests/order_cases.rs`` runs through ``ordercore::order`` too) through
:func:`~wraithguard.sort.engine.build_and_sort`, and the file itself against the generator, so a
rule changed in Python without regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "order_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_order_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_order_cases


@pytest.mark.parametrize("case", DOC["cases"], ids=[c["name"] for c in DOC["cases"]])
def test_the_customs_are_placed_among_the_curated_plugins(case: dict[str, Any]) -> None:
    rules = [(r["priority"], r["text"]) for r in case["rules"]]
    got = _generator().answer(rules, case["base"], case["subset"], case["masters"])
    for key, value in got.items():
        assert value == case[key], key


@pytest.mark.parametrize("case", DOC["cases"], ids=[c["name"] for c in DOC["cases"]])
def test_the_curated_order_is_never_reordered(case: dict[str, Any]) -> None:
    customs = {n.lower() for n in case["subset"]}
    frozen = [n for n in case["base"] if n.lower() not in customs]
    placed = [n for n in case["order"] if n in set(case["base"]) and n.lower() not in customs]
    assert placed == list(dict.fromkeys(frozen))


def test_the_case_file_is_what_the_generator_writes() -> None:
    assert _generator().cases() == DOC["cases"]
