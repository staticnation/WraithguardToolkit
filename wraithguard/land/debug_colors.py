"""Paint conflict severity into the merged terrain's vertex colours.

**What this is for.** A conflict report tells you cell (14, -4) had 103
contested vertices. It cannot tell you whether those vertices are under a road
you walk down every session or halfway up a cliff nobody approaches. Painting
the severity into the terrain itself answers that: load the merged plugin, walk
outside, and the ground is coloured where the merge had to choose.

Red is a major conflict, yellow a minor one, green a vertex exactly one mod
moved. Everything else is left alone.

**This deliberately produces a plugin you cannot play with.** The vertex
colours it writes are diagnostic, not scenery, and they replace whatever
lighting the mods intended. It is a switch for understanding a merge, and the
tool says so when it is on.

**Priority matters when a vertex is painted twice.** A vertex can be contested
by several pairs of plugins across successive folds. Red wins over yellow and
is never overwritten -- otherwise a later minor conflict would hide a major one
at the same spot, and hiding the worst case is the one thing a diagnostic must
not do.

Ported from ``repair/debugging.rs`` in Merged Lands (MIT, David Von Derau).
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Final

from wraithguard.land.merge import ConflictParams, Severity, average_delta
from wraithguard.tes3fields.landscape import LAND_SIZE

if TYPE_CHECKING:
    from wraithguard.land.diff import RelativeGrid

#: A vertex both plugins moved, whose compromise sits far from an intent.
MAJOR_COLOR: Final[tuple[int, int, int]] = (255, 0, 0)

#: A vertex both plugins moved, resolved close to both.
MINOR_COLOR: Final[tuple[int, int, int]] = (255, 255, 0)

#: A vertex exactly one plugin moved. No choice was needed.
MODIFIED_COLOR: Final[tuple[int, int, int]] = (0, 255, 0)

#: Painted over nothing. Left as whatever the merge produced.
UNMODIFIED_COLOR: Final[tuple[int, int, int]] = (0, 0, 0)


def _severity_at(
    left: RelativeGrid, right: RelativeGrid, x: int, y: int, params: ConflictParams
) -> tuple[int, int, int]:
    """Classify one vertex.

    Args:
        left: The accumulated merge.
        right: The plugin being folded in.
        x: Column.
        y: Row.
        params: Severity thresholds.

    Returns:
        The colour this vertex deserves.
    """
    if not right.has_difference(x, y):
        return UNMODIFIED_COLOR
    if not left.has_difference(x, y):
        return MODIFIED_COLOR

    worst = Severity.MINOR
    for one, two in zip(left.deltas_at(x, y), right.deltas_at(x, y)):
        if one == two:
            continue
        _, severity = average_delta(one, two, params)
        if severity is Severity.MAJOR:
            worst = Severity.MAJOR
            break
    return MAJOR_COLOR if worst is Severity.MAJOR else MINOR_COLOR


def paint_conflicts(
    colors: list[int],
    left: RelativeGrid | None,
    right: RelativeGrid | None,
    params: ConflictParams | None = None,
) -> int:
    """Paint one fold's conflicts into a flat vertex-colour grid.

    Call once per plugin folded into a cell, with the accumulated merge and
    the plugin's own differences. Colours accumulate across calls under the
    priority rule in the module docstring.

    Args:
        colors: A flat, interleaved RGB grid, modified in place.
        left: The accumulated merge, or ``None`` on the first plugin.
        right: The plugin's differences, or ``None`` if it changed nothing.
        params: Severity thresholds, or ``None`` for the defaults.

    Returns:
        How many vertices were painted.

    Raises:
        ValueError: If the colour grid is the wrong length.
    """
    expected = LAND_SIZE * LAND_SIZE * 3
    if len(colors) != expected:
        raise ValueError(f"expected {expected} colour components, got {len(colors)}")
    if right is None:
        return 0

    thresholds = params if params is not None else ConflictParams()
    painted = 0

    for y in range(LAND_SIZE):
        for x in range(LAND_SIZE):
            if left is None:
                colour = MODIFIED_COLOR if right.has_difference(x, y) else UNMODIFIED_COLOR
            else:
                colour = _severity_at(left, right, x, y, thresholds)
            if colour == UNMODIFIED_COLOR:
                continue

            base = (y * LAND_SIZE + x) * 3
            current = (colors[base], colors[base + 1], colors[base + 2])
            # Red is never demoted. A later minor conflict at the same vertex
            # would otherwise conceal a major one, and concealing the worst
            # case defeats the purpose of the switch.
            if current == MAJOR_COLOR and colour != MAJOR_COLOR:
                continue
            if current == colour:
                continue
            colors[base], colors[base + 1], colors[base + 2] = colour
            painted += 1

    return painted


def blank_colors() -> list[int]:
    """A flat vertex-colour grid with nothing painted.

    Returns:
        65 x 65 x 3 zeroes.
    """
    return [0] * (LAND_SIZE * LAND_SIZE * 3)
