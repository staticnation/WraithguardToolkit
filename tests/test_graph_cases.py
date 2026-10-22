"""The shared sort-graph cases (``tests/fixtures/graph_cases.json``, which
``viewer-shell/ordercore/tests/graph_cases.rs`` runs through ``ordercore::graph`` too) through
:func:`~wraithguard.sort.graph.is_master_file`, :func:`~wraithguard.sort.graph.would_create_cycle`
and :func:`~wraithguard.sort.engine.build_edges`, and the file itself against the generator, so
a rule changed in Python without regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

from wraithguard.sort.graph import is_master_file, would_create_cycle

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "graph_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_graph_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_graph_cases


@pytest.mark.parametrize("case", DOC["masters"], ids=[repr(c["name"]) for c in DOC["masters"]])
def test_a_master_file_is_told_by_its_extension(case: dict[str, Any]) -> None:
    assert is_master_file(case["name"]) == case["is_master"]


@pytest.mark.parametrize("case", DOC["cycles"], ids=[c["name"] for c in DOC["cycles"]])
def test_an_edge_that_closes_a_cycle_is_found(case: dict[str, Any]) -> None:
    got = would_create_cycle(case["adjacency"], case["start"], case["target"])
    assert got == case["closes_cycle"]


@pytest.mark.parametrize("case", DOC["graphs"], ids=[c["name"] for c in DOC["graphs"]])
def test_the_graph_is_built_from_rules_masters_and_the_cfg(case: dict[str, Any]) -> None:
    rules = [(r["priority"], r["text"]) for r in case["rules"]]
    got = _generator().answer(rules, case["base"], case["subset"], case["masters"])
    for key, value in got.items():
        assert value == case[key], key


def test_the_case_file_is_what_the_generator_writes() -> None:
    out = _generator().cases()
    assert out["masters"] == DOC["masters"]
    assert out["cycles"] == DOC["cycles"]
    assert out["graphs"] == DOC["graphs"]
