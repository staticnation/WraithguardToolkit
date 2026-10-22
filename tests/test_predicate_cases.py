"""The shared predicate cases (``tests/fixtures/predicate_cases.json``, which
``viewer-shell/ordercore/tests/predicate_cases.rs`` runs through ``ordercore::predicates`` too)
through the Python tokeniser, parser, ``format_version`` and ``check_predicates`` (over real plugin
files it writes), and the file itself against the generator, so a rule changed in Python without
regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "predicate_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_predicate_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_predicate_cases


def test_expressions_are_read_as_recorded() -> None:
    gen = _generator()
    wrong = [c["text"] for c in DOC["expressions"] if gen.expression_case(c["text"]) != c]
    assert not wrong, wrong[:10]


def test_versions_are_formatted_as_recorded() -> None:
    from wraithguard.versions import format_version

    wrong = [
        c["version"] for c in DOC["versions"] if format_version(c["version"]) != c["formatted"]
    ]
    assert not wrong, wrong


@pytest.mark.parametrize("case", DOC["checks"], ids=[c["name"] for c in DOC["checks"]])
def test_rule_text_warns_as_recorded(case: dict[str, Any]) -> None:
    disk = None if case["disk"] is None else {n: (v[0], v[1]) for n, v in case["disk"].items()}
    got = _generator().answer(case["rules"], case["order"], case["origins"], disk)
    for key, value in got.items():
        assert value == case[key], key


def test_the_case_file_is_what_the_generator_writes() -> None:
    out = _generator().cases()
    assert out["expressions"] == DOC["expressions"]
    assert out["versions"] == DOC["versions"]
    assert out["checks"] == DOC["checks"]
