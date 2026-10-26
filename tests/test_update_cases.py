"""The shared download cases (``tests/fixtures/update_cases.json``, which
``viewer-shell/ordercore/tests/update_cases.rs`` runs through ``ordercore::updates`` too) through
:mod:`wraithguard.net.updaters` with a scripted network and a fixed clock, and the file itself
against the generator, so a rule changed in Python without regenerating them fails here."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "update_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_update_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_update_cases


def test_every_url_is_judged_as_recorded() -> None:
    gen = _generator()
    wrong = [c["url"] for c in DOC["urls"] if gen.url_case(c["url"]) != c]
    assert not wrong, wrong[:10]


def test_hostile_urls_are_refused_and_never_reach_the_network() -> None:
    by_url = {c["url"]: c for c in DOC["urls"]}
    for url in (
        "file:///etc/passwd",
        "ftp://example.com/x",
        "data:text/plain,hello",
        "https://",
        "",
    ):
        assert by_url[url]["refused"], url
        assert not by_url[url].get("reached"), url


@pytest.mark.parametrize("flow", DOC["flows"], ids=[f["name"] for f in DOC["flows"]])
def test_an_update_does_as_recorded(flow: dict[str, Any]) -> None:
    got = _generator().flow_case(flow)
    for key, value in got.items():
        if key == "report" and flow.get("parser_detail"):
            # a JSON parser's own words about its syntax error are its own (ordercore's are
            # serde_json's), so they are masked on both sides, as the Rust test masks them
            assert _masked(value) == _masked(flow[key]), key
        else:
            assert value == flow[key], key


def _masked(report: list[str]) -> list[str]:
    return [
        re.sub(r"(not a cfg-generator response \().*(\) \(is ')", r"\1...\2", line)
        for line in report
    ]


def test_ages_are_as_recorded() -> None:
    gen = _generator()
    for case in DOC["ages"]:
        assert gen.ages_case(case["files"]) == {k: case[k] for k in ("files", "now", "ages")}, case[
            "name"
        ]


def test_the_case_file_is_what_the_generator_writes() -> None:
    out = _generator().cases()
    for key in ("urls", "fetches", "constants", "tails", "flows", "ages"):
        assert out[key] == DOC[key], key
