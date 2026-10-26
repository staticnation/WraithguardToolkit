"""The shared input cases (``tests/fixtures/input_cases.json``, which
``viewer-shell/ordercore/tests/input_cases.rs`` runs through ``ordercore::inputs`` too) through the
Python that gathers a sort's inputs, as it is called (so through ordercore when the native module
answers), and the file itself against the generator, so a rule changed in Python without
regenerating them fails here."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests" / "fixtures" / "input_cases.json"
DOC: dict[str, Any] = json.loads(FILE.read_text(encoding="utf-8"))


def _generator() -> Any:
    sys.path.insert(0, str(ROOT))
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import gen_input_cases
    finally:
        sys.path.remove(str(ROOT / "tools"))
    return gen_input_cases


def _wrong(cases: list[dict[str, Any]], answer: Any) -> list[str]:
    return [c.get("name", str(i)) for i, c in enumerate(cases) if answer(c) != c]


def test_subset_lines_read_as_recorded() -> None:
    gen = _generator()
    assert not _wrong(DOC["lines"], lambda c: gen.lines_case(c["name"], c["lines"]))


def test_the_subset_files_toml_reads_as_recorded() -> None:
    gen = _generator()
    wrong = _wrong(
        DOC["subset_tomls"],
        lambda c: gen.subset_toml_case(c["name"], c["text"], bool(c.get("declined"))),
    )
    assert not wrong


def test_a_customizations_toml_reads_as_recorded() -> None:
    gen = _generator()
    wrong = _wrong(
        DOC["customizations"],
        lambda c: gen.customizations_case(c["name"], c["text"], bool(c.get("declined"))),
    )
    assert not wrong, wrong[:10]


def test_grass_and_folders_as_recorded() -> None:
    gen = _generator()
    assert not _wrong(DOC["holdbacks"], lambda c: gen.holdback_case(c["subset"], c["groundcover"]))
    assert not _wrong(
        DOC["scan_dirs"], lambda c: gen.scan_dirs_case(c["data_order"], c["raw"], c["inserts"])
    )


def test_a_lists_data_paths_as_recorded() -> None:
    gen = _generator()
    wrong = _wrong(DOC["data_paths"], lambda c: gen.data_paths_case(c["name"], c["text"], c["cfg"]))
    assert not wrong, wrong[:10]


def test_the_cfgs_orphans_as_recorded() -> None:
    gen = _generator()
    keys = ("subset", "base_order", "data_order", "curated", "needs_cleaning", "declared")
    keys += ("groundcover", "declared_data", "tails")
    assert not _wrong(DOC["orphans"], lambda c: gen.orphan_case(c["name"], {k: c[k] for k in keys}))


@pytest.mark.parametrize("case", DOC["trees"], ids=[c["name"] for c in DOC["trees"]])
def test_a_mods_folder_as_recorded(case: dict[str, Any]) -> None:
    gen = _generator()
    q = {
        k: case[k]
        for k in ("scan", "data_order", "final_order", "inserts", "dirs", "active", "origins")
    }
    assert gen.tree_case(case["name"], case["files"], q) == case


def test_the_case_file_is_what_the_generator_writes() -> None:
    out = json.loads(json.dumps(_generator().cases(), ensure_ascii=False))
    for key, value in out.items():
        assert value == DOC[key], key
