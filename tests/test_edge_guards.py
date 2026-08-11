"""Small guard clauses and edge branches left uncovered across the tree.

Each of these is a one-liner -- an error guard, an early return, a property, a
revisit-skip -- that a module's main tests never happened to reach. They are
grouped here rather than scattered because none belongs to a larger feature;
together they close the last statement or two in several near-complete files.
"""

from __future__ import annotations

import pytest

from wraithguard.land.cleaning import CleaningReport
from wraithguard.land.curvature import curvature_map
from wraithguard.patch.status import conflict_this
from wraithguard.sort.graph import would_create_cycle
from wraithguard.viz.geometry import parse_grid
from wraithguard.viz.terrain3d import _sample


def test_cleaning_report_dropped_sums_both_reasons() -> None:
    """``dropped`` totals the unmodified and single-source cells removed."""
    assert CleaningReport(unmodified=2, single_source=3).dropped == 5


def test_curvature_map_rejects_a_too_small_grid() -> None:
    """Curvature needs neighbours, so a sub-2x2 grid is an error."""
    with pytest.raises(ValueError, match="2x2"):
        curvature_map([[1.0]])


def test_parse_grid_rejects_a_non_string_id() -> None:
    """A record id that is not a string carries no coordinates."""
    assert parse_grid(123) is None


def test_sample_with_a_trivial_stride_copies_the_grid() -> None:
    """Stride 1 means no reduction -- the grid comes back unchanged."""
    grid = [[1.0, 2.0], [3.0, 4.0]]
    assert _sample(grid, 1) == grid


def test_conflict_this_of_nothing_is_empty() -> None:
    """No values in play yields an empty result, not an index error."""
    assert conflict_this([]) == []


def test_would_create_cycle_is_true_when_target_reaches_start() -> None:
    """The proposed edge closes a loop: target already reaches start."""
    assert would_create_cycle({"target": ["start"]}, "start", "target") is True


def test_would_create_cycle_skips_a_node_reached_twice() -> None:
    """A diamond reaches one node by two paths; the second visit is skipped."""
    adjacency = {"target": ["a", "b"], "a": ["c"], "b": ["c"]}
    assert would_create_cycle(adjacency, "z", "target") is False
