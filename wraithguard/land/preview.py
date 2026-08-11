"""Preview what a conflict strategy would do to one contested cell.

The merge decides a whole load order at once. This answers the smaller,
interactive question a user asks *before* writing a ``.mergedlands.toml``: given
the plugins that edit this one cell, what does each strategy produce? Seeing
``Overwrite`` against ``Resolve`` on the actual terrain is the thing the original
tool's conflict images gave and the number in a diff does not.

The first plugin in load order stands in for the reference the others are
measured against -- for a vanilla cell that is the master, and for new land it
is the mod that laid the ground down. Either way it is the same base the real
merge folds the later plugins onto, so a preview built this way matches what the
run would do for the cell.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from wraithguard.land.diff import LandData, RelativeGrid
from wraithguard.land.merge import ConflictStrategy, merge_layer
from wraithguard.tes3fields.landscape import LAND_SIZE

if TYPE_CHECKING:
    from collections.abc import Sequence


def _flatten(grid: Sequence[Sequence[float]]) -> list[int]:
    """Flatten a 65x65 height grid to the integer values the merge works in.

    Args:
        grid: A 65x65 grid of absolute heights.

    Returns:
        The heights row-major, as ints.

    Raises:
        ValueError: If the grid is not 65x65.
    """
    if len(grid) != LAND_SIZE or any(len(row) != LAND_SIZE for row in grid):
        raise ValueError(f"each terrain grid must be {LAND_SIZE}x{LAND_SIZE}")
    return [int(value) for row in grid for value in row]


def merge_preview(
    grids: Sequence[Sequence[Sequence[float]]],
    strategy: ConflictStrategy,
) -> list[list[float]]:
    """Merge several plugins' heights for one cell under one strategy.

    Args:
        grids: Per-plugin 65x65 absolute-height grids, in load order. The first
            is the base every later one is measured against; the rest are folded
            onto it in order, exactly as the real merge folds a load order onto
            its reference.
        strategy: The conflict strategy applied where two plugins moved the same
            vertex. With fewer than two *editing* plugins there is no conflict,
            so every strategy returns the same surface -- the comparison only
            differs for a cell two or more plugins genuinely contest.

    Returns:
        The merged 65x65 heights.

    Raises:
        ValueError: If no grids are given, or one is not 65x65.
    """
    grids = list(grids)
    if not grids:
        raise ValueError("no terrain to merge")
    reference = _flatten(grids[0])
    merged = RelativeGrid(reference, LAND_SIZE, 1)  # zero delta: the base itself
    for grid in grids[1:]:
        incoming = RelativeGrid.from_difference(reference, _flatten(grid), LAND_SIZE, 1)
        merged, _ = merge_layer(LandData.VERTEX_HEIGHTS, merged, incoming, strategy)
    flat = merged.to_flat()
    return [[float(flat[y * LAND_SIZE + x]) for x in range(LAND_SIZE)] for y in range(LAND_SIZE)]


#: The strategies a preview compares, in the order a chooser shows them. ``Auto``
#: is left out because it resolves to one of these (``Overwrite``); ``Curvature``
#: is included because seeing it against ``Resolve`` is exactly when it helps.
PREVIEW_STRATEGIES: tuple[ConflictStrategy, ...] = (
    ConflictStrategy.OVERWRITE,
    ConflictStrategy.RESOLVE,
    ConflictStrategy.IGNORE,
    ConflictStrategy.CURVATURE,
)
