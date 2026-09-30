"""wraithguard.parallel: reads side by side, results in the order asked."""

from __future__ import annotations

import threading
import time

import pytest

from wraithguard.parallel import read_all, workers


def test_results_come_back_in_order_whatever_finishes_first() -> None:
    # The early items sleep longest, so they finish last.
    items = list(range(12))

    def slow_first(i: int) -> int:
        time.sleep(0.002 * (12 - i))
        return i * i

    assert read_all(items, slow_first) == [i * i for i in items]


def test_it_really_runs_side_by_side() -> None:
    seen: set[int] = set()
    lock = threading.Lock()

    def note(_i: int) -> None:
        with lock:
            seen.add(threading.get_ident())
        time.sleep(0.01)

    read_all(range(16), note, cap=4)
    if workers(16, 4) > 1:
        assert len(seen) > 1


def test_an_error_is_raised_as_the_loop_would() -> None:
    def boom(i: int) -> int:
        if i == 3:
            raise ValueError("bad plugin")
        return i

    with pytest.raises(ValueError, match="bad plugin"):
        read_all(range(6), boom)


def test_small_and_empty_inputs() -> None:
    assert read_all([], lambda x: x) == []
    assert read_all(["a"], str.upper) == ["A"]
    assert workers(0) == 1
    assert workers(3) <= 3
