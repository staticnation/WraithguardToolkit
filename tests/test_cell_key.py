"""A cell named one way: the shared cell cases (``tests/fixtures/cell_cases.json``, which
``viewer-shell/ordercore`` runs too) through :mod:`wraithguard.cells`, and the Editor's
own readers of a cell agreeing with them."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.cells import cell_key, grid, pool_key
from wraithguard.patch.editor import EditorSession, exterior_grid

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "cell_cases.json").read_text(encoding="utf-8")
)["cases"]


@pytest.mark.parametrize("case", CASES, ids=[c["in"] for c in CASES])
def test_case(case: dict[str, Any]) -> None:
    got = cell_key(case["in"])
    if case["fold"] is None:
        assert got is None
        with pytest.raises(ValueError):
            pool_key(case["in"])
        return
    assert got is not None
    want_grid = tuple(case["grid"]) if case["grid"] else None
    assert (got["fold"], got["spec"], got["pool"], got["grid"]) == (
        case["fold"],
        case["spec"],
        case["pool"],
        want_grid,
    )
    assert pool_key(case["in"]) == case["pool"]
    assert grid(case["in"]) == want_grid
    # The Editor's readers: the pool's key, the grid, a path grid's key.
    assert EditorSession._cell_key(case["in"]) == case["pool"]
    assert exterior_grid(case["in"]) == want_grid
    assert EditorSession.pathgrid_key(case["in"]) == (case["pool"], want_grid)


@pytest.mark.parametrize(
    "case", [c for c in CASES if c["fold"] and c["grid"] is None], ids=lambda c: c["in"]
)
def test_an_interior_folds_as_the_pool_compares(case: dict[str, Any]) -> None:
    """The pool compares names with ``str.lower()``: the shared fold must say the same."""
    got = cell_key(case["in"])
    assert got is not None
    assert got["fold"] == "int:" + got["pool"].lower()
