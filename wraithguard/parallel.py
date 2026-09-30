"""Reading many plugins at once.

Most of what the toolkit does starts by reading every plugin in the load order, and
most of that reading is the Rust parser (``wraithguard_native``), which lets go of the
interpreter while it works - and the builds are free-threaded Python besides. So the
reads run side by side on a small pool, and the results come back **in the order
asked**, so everything that folds them (last plugin wins, first writer wins) does
exactly what it did when the loop read them one after another.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor
from typing import TYPE_CHECKING, TypeVar

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable

T = TypeVar("T")
R = TypeVar("R")

#: More than this gains nothing: the disk and the memory bus are shared.
MAX_WORKERS = 8


def workers(n_items: int, cap: int | None = None) -> int:
    """How many threads to read ``n_items`` with.

    Args:
        n_items: How many things there are to read.
        cap: A lower ceiling than :data:`MAX_WORKERS`, if the caller wants one.

    Returns:
        At least 1, at most the CPU count, the cap and the item count.
    """
    top = min(MAX_WORKERS, cap or MAX_WORKERS, os.cpu_count() or 1)
    return max(1, min(top, n_items))


def read_all(items: Iterable[T], fn: Callable[[T], R], cap: int | None = None) -> list[R]:
    """``[fn(x) for x in items]``, run on a thread pool, in the order of ``items``.

    An exception in ``fn`` is raised here, as the loop would have raised it; a caller
    that wants a bad plugin skipped catches inside ``fn``.

    Args:
        items: What to read.
        fn: Reads one.
        cap: A lower worker ceiling (see :func:`workers`).

    Returns:
        The results, one per item, in order.
    """
    seq = list(items)
    n = workers(len(seq), cap)
    if n <= 1:
        return [fn(x) for x in seq]
    with ThreadPoolExecutor(max_workers=n, thread_name_prefix="wg-read") as pool:
        return list(pool.map(fn, seq))
