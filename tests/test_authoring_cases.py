"""The shared rule-maker cases (``tests/fixtures/authoring_cases.json``, which
``viewer-shell/ordercore/tests/authoring_cases.rs`` runs through ``ordercore::authoring`` and
``ordercore::derive`` too) through the Python rule maker, the proposal deriver and the user rule
file writers, and the file itself against the generator, so a rule changed in Python without
regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "authoring_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_authoring_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_authoring_cases


def test_citations_are_formatted_as_recorded() -> None:
    from wraithguard.rules.authoring import format_ref

    assert [{"ref": c["ref"], "formatted": format_ref(c["ref"])} for c in DOC["refs"]] == DOC[
        "refs"
    ]


def test_rules_render_and_validate_as_recorded() -> None:
    gen = _generator()
    wrong = [c["name"] for c in DOC["rules"] if gen.rule_case(c["name"], c["rule"]) != c]
    assert not wrong, wrong[:10]


def test_proposals_are_derived_as_recorded() -> None:
    gen = _generator()
    keys = ("name", "masters", "active", "conflicts", "minimum")
    wrong = [c["name"] for c in DOC["proposals"] if gen.proposal_case(*(c[k] for k in keys)) != c]
    assert not wrong, wrong[:10]


@pytest.mark.parametrize("case", DOC["files"], ids=[c["name"] for c in DOC["files"]])
def test_a_rule_lands_in_the_users_file_as_recorded(case: dict[str, Any]) -> None:
    what = {k: case[k] for k in ("rule", "user") if k in case}
    existing = None if case["existing"] is None else case["existing"].encode("utf-8")
    assert _generator().file_case(case["name"], existing, what) == case


def test_a_rule_with_errors_is_never_written() -> None:
    refused = [c for c in DOC["files"] if "refused" in c]
    assert refused
    for case in refused:
        assert case["after"] == case["existing"]


def test_the_case_file_is_what_the_generator_writes() -> None:
    out = json.loads(json.dumps(_generator().cases(), ensure_ascii=False))
    for key in ("refs", "rules", "proposals", "files", "frozen"):
        assert out[key] == DOC[key], key
