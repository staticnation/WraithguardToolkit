"""The shared MOMW cases (``tests/fixtures/momw_cases.json``, which
``viewer-shell/ordercore/tests/momw_cases.rs`` runs through ``ordercore::momw`` too) through
:mod:`wraithguard.momw`, and the file itself against the generator, so a rule changed in
Python without regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

from wraithguard.configurator.cfglines import curated_covers
from wraithguard.momw import (
    base_order_matches_yml,
    curated_for_list,
    needs_cleaning_set,
    parse_plugin_order_text,
    parse_plugin_order_yml,
    post_sort_findings,
)

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "momw_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _names(section: str) -> list[str]:
    return [c["name"] for c in DOC[section]]


@pytest.mark.parametrize("case", DOC["yml"], ids=_names("yml"))
def test_the_hand_reader_reads_a_yml_as_the_case_says(case: dict[str, Any]) -> None:
    assert [dict(e) for e in parse_plugin_order_text(case["text"])] == case["entries"]


@pytest.mark.parametrize("case", DOC["yml"], ids=_names("yml"))
def test_pyyaml_reads_the_same_except_where_the_case_file_says_it_does_not(
    case: dict[str, Any], tmp_path: Path
) -> None:
    yaml = pytest.importorskip("yaml")
    path = tmp_path / "plugin-order.yml"
    path.write_bytes(case["text"].encode("utf-8"))
    try:
        via_yaml: object = [dict(e) for e in parse_plugin_order_yml(path)]
    except yaml.YAMLError as exc:  # PyYAML raises on a malformed file: a difference
        via_yaml = exc
    same = via_yaml == case["entries"]
    assert same == (case["name"] not in DOC["yaml_differs"])


@pytest.mark.parametrize("case", DOC["lists"], ids=_names("lists"))
def test_a_list_picks_its_plugins(case: dict[str, Any]) -> None:
    got_set, got_order = curated_for_list(case["entries"], case["list"])
    assert sorted(got_set) == case["set"]
    assert got_order == case["order"]


@pytest.mark.parametrize("case", DOC["cleaning"], ids=_names("cleaning"))
def test_the_needs_cleaning_set(case: dict[str, Any]) -> None:
    assert sorted(needs_cleaning_set(case["entries"])) == case["set"]


@pytest.mark.parametrize("case", DOC["drift"], ids=_names("drift"))
def test_list_order_drift(case: dict[str, Any]) -> None:
    assert base_order_matches_yml(case["base"], case["curated"]) == case["warnings"]


@pytest.mark.parametrize("case", DOC["covers"], ids=_names("covers"))
def test_a_curated_list_covers_a_plugin(case: dict[str, Any]) -> None:
    got = curated_covers(case["plugin"], set(case["curated"]), set(case["needs_cleaning"]))
    assert got == case["expect"]


@pytest.mark.parametrize("case", DOC["findings"], ids=_names("findings"))
def test_the_post_sort_findings(case: dict[str, Any]) -> None:
    got = post_sort_findings(
        case["final_order"],
        case["base"],
        case["entries"],
        set(case["curated"]),
        case["curated_order"],
        set(case["declared"]),
        case["list"],
    )
    assert got == case["warnings"]


def test_the_case_file_is_what_the_generator_writes() -> None:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_momw_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    assert gen_momw_cases.cases() == {k: v for k, v in DOC.items() if k != "_about"}
