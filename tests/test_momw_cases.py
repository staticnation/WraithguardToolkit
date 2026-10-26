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

from wraithguard import ordercore_bridge
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
def test_a_yml_is_read_or_refused_as_the_case_says(case: dict[str, Any], tmp_path: Path) -> None:
    if ordercore_bridge.native("order_momw_parse") is None:
        pytest.skip(
            "ordercore's reader is not answering (no native module, or WG_ORDERCORE=python)"
        )
    path = tmp_path / "plugin-order.yml"
    path.write_bytes(case["text"].encode("utf-8"))
    if case.get("refused"):
        with pytest.raises(ValueError, match=r"\S"):
            parse_plugin_order_yml(path)
        with pytest.raises(ValueError, match=r"\S"):
            parse_plugin_order_text(case["text"])
    else:
        assert [dict(e) for e in parse_plugin_order_yml(path)] == case["entries"]
        assert [dict(e) for e in parse_plugin_order_text(case["text"])] == case["entries"]


def test_a_refusal_says_where_on_one_line(tmp_path: Path) -> None:
    if ordercore_bridge.native("order_momw_parse") is None:
        pytest.skip(
            "ordercore's reader is not answering (no native module, or WG_ORDERCORE=python)"
        )
    path = tmp_path / "plugin-order.yml"
    path.write_bytes(b"- file_name: A.esp\n  file_name: B.esp\n")
    with pytest.raises(ValueError, match=r"^line 2 column 3: duplicate mapping key: file_name$"):
        parse_plugin_order_yml(path)


def test_momws_own_file_reads_as_the_line_reader_reads_it() -> None:
    if ordercore_bridge.native("order_momw_parse") is None:
        pytest.skip(
            "ordercore's reader is not answering (no native module, or WG_ORDERCORE=python)"
        )
    from wraithguard.momw import _parse_plugin_order_lines

    path = ROOT / "testdata" / "plugin-order.yml"
    want = _parse_plugin_order_lines(path.read_text(encoding="utf-8"))
    assert parse_plugin_order_yml(path) == want
    assert len(want) > 100


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
