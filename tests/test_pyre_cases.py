"""The shared regular-expression cases (``tests/fixtures/pyre_cases.json``, which
``viewer-shell/ordercore/tests/pyre_cases.rs`` runs through ``ordercore::pyre`` too) against
Python's ``re``, so a case file that no longer says what ``re`` says fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "pyre_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_pyre_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_pyre_cases


def test_every_case_says_what_re_says() -> None:
    gen = _generator()
    wrong = []
    for case in DOC["cases"]:
        got = gen.answer(case["pattern"], case["ignore_case"], case["multiline"], case["text"])
        if any(got[key] != case[key] for key in got):
            wrong.append((case["pattern"], case["text"]))
    assert not wrong, wrong[:10]


def test_the_expressions_the_rule_code_compiles_are_the_ones_listed() -> None:
    """The eight ``real`` patterns are the rule code's own, not a copy that drifted."""
    from wraithguard.rules import expressions, parser, predicates

    real = {c["pattern"] for c in DOC["cases"] if c["real"]}
    ours = {
        expressions._FUNC_FORM_RE.pattern,
        expressions._KEYWORD_RE.pattern,
        expressions._MESSAGE_RE.pattern,
        parser.TOP_RE.pattern,
        predicates._re_ver_fun.pattern,
        predicates._re_size_fun.pattern,
        predicates._re_desc_fun.pattern,
        predicates._re_mwselua_fun.pattern,
    }
    assert real == ours


def test_the_case_file_is_what_the_generator_writes() -> None:
    assert _generator().cases() == DOC["cases"]
