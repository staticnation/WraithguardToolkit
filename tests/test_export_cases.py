"""The shared export cases (``tests/fixtures/export_cases.json``, which
``viewer-shell/ordercore/tests/export_cases.rs`` runs through ``ordercore::export`` too) through
the Python export code (the ``data=`` line helpers, ``insert_data_paths``,
``generate_customizations_toml``, ``read_cfg`` / ``render_cfg_lines`` and ``write_plan`` over a
temp folder with a fixed clock), and the file itself against the generator, so a rule changed in
Python without regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "export_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT))
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_export_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_export_cases


def test_the_line_helpers_answer_as_recorded() -> None:
    assert _generator().lines_cases() == DOC["lines"]


def test_inserts_land_as_recorded() -> None:
    assert _generator().insert_cases() == DOC["inserts"]


def test_the_customizations_toml_is_written_as_recorded() -> None:
    gen = _generator()
    args_keys = [k for k in DOC["tomls"][0] if k not in ("name", "text", "warnings")]
    wrong = [
        c["name"]
        for c in DOC["tomls"]
        if gen.toml_case(c["name"], {k: c[k] for k in args_keys}) != c
    ]
    assert not wrong, wrong[:10]


def test_cfgs_are_read_and_rendered_as_recorded() -> None:
    gen = _generator()
    assert gen.cfg_cases() == DOC["cfgs"]
    wrong = [
        c["name"]
        for c in DOC["renders"]
        if gen.render_case(c["name"], c["cfg"], c["plan"], c["final_order"], c["data_order"]) != c
    ]
    assert not wrong, wrong


def test_the_removal_rules_decide_as_recorded() -> None:
    assert _generator().removal_cases() == DOC["removals"]


@pytest.mark.parametrize("case", DOC["writes"], ids=[c["name"] for c in DOC["writes"]])
def test_an_export_leaves_the_recorded_files(case: dict[str, Any]) -> None:
    got = _generator().write_case(
        case["name"], case["cfg"], case["toml"], case["plan"], case["opts"]
    )
    assert got == case


def test_the_case_file_is_what_the_generator_writes() -> None:
    out = json.loads(json.dumps(_generator().cases(), ensure_ascii=False))
    for key in ("lines", "inserts", "tomls", "cfgs", "renders", "removals", "writes"):
        assert out[key] == DOC[key], key
