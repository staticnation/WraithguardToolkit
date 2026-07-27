"""Colour ramps for the conflict visualisations.

Two ramps, chosen for what they have to communicate rather than for looks:

*Divergence* (height deltas) is symmetric around zero, because "this mod raised
the ground" and "this mod lowered it" are equally interesting and neither is
the default. Red is up and blue is down, with near-zero left almost neutral so
the eye is drawn to real movement rather than to rounding.

*Severity* (conflict counts) runs cool to hot, following the convention
``merged_lands`` established for TES3 land conflicts -- green is fine, yellow
is worth a look, red wants attention. Matching an existing tool's language
matters more here than picking a nicer palette: people read both.

Both ramps are computed rather than tabulated, so they stay smooth at any
number of steps and no lookup table has to be kept in sync with a legend.
"""

from __future__ import annotations

from collections.abc import Sequence
from itertools import pairwise
from typing import Final

#: Neutral fill for a cell with data but nothing to report.
NEUTRAL = "#2c313a"

#: Outline for anything involving the user's own mods, matching the GUI's
#: orange "your custom mod" marker in the field-diff window.
MINE = "#ff9d5c"


def _clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    """Constrain a value to a range.

    Args:
        value: The value to clamp.
        low: Lower bound.
        high: Upper bound.

    Returns:
        ``value`` limited to ``[low, high]``.
    """
    return max(low, min(high, value))


def _hex(red: float, green: float, blue: float) -> str:
    """Format three 0-1 channel values as a CSS hex colour.

    Args:
        red: Red channel, 0-1.
        green: Green channel, 0-1.
        blue: Blue channel, 0-1.

    Returns:
        A ``#rrggbb`` string.
    """
    return f"#{round(_clamp(red) * 255):02x}{round(_clamp(green) * 255):02x}{round(_clamp(blue) * 255):02x}"


def divergence(value: float, scale: float) -> str:
    """Map a signed value to a blue-neutral-red divergence colour.

    Args:
        value: The signed quantity, e.g. a height delta in world units.
        scale: The magnitude that saturates the ramp. Values beyond it clamp
            rather than wrap, so one extreme vertex cannot wash out the rest.

    Returns:
        A ``#rrggbb`` string: blue for negative, red for positive, dark
        neutral at zero.
    """
    if scale <= 0:
        return NEUTRAL
    t = _clamp(abs(value) / scale)
    # Ease the ramp so small deltas stay visible instead of vanishing into the
    # neutral end -- a 20-unit nudge matters and would otherwise be invisible
    # beside a 2000-unit cliff.
    t = t**0.6
    if value >= 0:
        return _hex(0.17 + 0.78 * t, 0.19 - 0.08 * t, 0.23 - 0.15 * t)
    return _hex(0.17 - 0.13 * t, 0.19 + 0.35 * t, 0.23 + 0.72 * t)


def severity(count: int, worst: int) -> str:
    """Map a conflict count to a green-yellow-red severity colour.

    The ramp is **linear**, deliberately. An earlier version used a square root
    to stop a few extreme cells flattening everything else to green -- and it
    did the opposite of what was wanted: with a busy load order it pushed
    ordinary cells (3 conflicts out of a worst of 30) straight into yellow, so
    the whole map read as "everything is on fire" and nothing stood out. That
    was only visible by rendering it and looking.

    The skew is real, but the fix belongs in the *scale*, not the curve: pass a
    high percentile as ``worst`` (see
    :func:`~mlox_subset.viz.conflictmap.build_conflict_map`) so one pathological
    cell clamps instead of rescaling everyone.

    Args:
        count: Conflicts on this cell.
        worst: The count that saturates the ramp.

    Returns:
        A ``#rrggbb`` string, or :data:`NEUTRAL` when there is nothing to show.
    """
    if count <= 0 or worst <= 0:
        return NEUTRAL
    return _ramp(_clamp(count / worst), _SEVERITY_STOPS)


#: The severity ramp as explicit stops, interpolated between. Five stops rather
#: than the original three: with only green -> yellow -> red, the whole middle of
#: a busy map collapsed into a narrow yellow band and neighbouring cells with
#: genuinely different counts looked identical. Adding a blue-green floor and an
#: orange shoulder widens the *discriminable* range without changing the
#: convention (cool = fine, hot = wants attention) that ``merged_lands``
#: established and that people already read.
_SEVERITY_STOPS: Final[tuple[tuple[float, float, float, float], ...]] = (
    (0.00, 0.18, 0.45, 0.52),  # teal -- a single quiet overlap
    (0.25, 0.35, 0.70, 0.35),  # green
    (0.50, 0.85, 0.82, 0.25),  # yellow
    # The shoulder's red sits just *below* the final red rather than above it.
    # A brighter orange looks marginally punchier in isolation, but it makes the
    # red channel fall over the last quarter of the ramp, and "more conflicts
    # never renders cooler" is an invariant this map is read against (and
    # `tests/test_viz.py::test_severity_is_monotonic` asserts). The hue is
    # unchanged; only the channel ordering is fixed.
    (0.75, 0.86, 0.55, 0.15),  # orange
    (1.00, 0.87, 0.16, 0.16),  # red -- the busiest cells on this map
)

#: The coverage ramp, for "how many mods touch this cell". Distinct from
#: severity on purpose: coverage is not badness -- ten mods touching a cell is
#: normal in a big load order -- so it runs cool blue through violet to amber
#: rather than green-to-red, and cannot be mistaken for the conflict map at a
#: glance. Seven stops, because coverage counts spread much wider than conflict
#: counts do.
_COVERAGE_STOPS: Final[tuple[tuple[float, float, float, float], ...]] = (
    (0.00, 0.18, 0.29, 0.39),  # slate -- exactly one mod
    (0.17, 0.16, 0.38, 0.55),  # blue
    (0.34, 0.20, 0.50, 0.62),  # cyan-blue
    (0.50, 0.36, 0.44, 0.68),  # periwinkle
    (0.67, 0.55, 0.40, 0.62),  # violet
    (0.84, 0.72, 0.44, 0.40),  # rose-amber
    (1.00, 0.85, 0.62, 0.24),  # amber -- the most contested cells
)


def _ramp(t: float, stops: Sequence[tuple[float, float, float, float]]) -> str:
    """Interpolate a multi-stop colour ramp.

    Args:
        t: Position along the ramp, 0-1 (clamped by the caller).
        stops: Ascending ``(position, red, green, blue)`` tuples, each channel
            0-1. The first and last positions must be 0 and 1.

    Returns:
        A ``#rrggbb`` string.
    """
    for (p0, r0, g0, b0), (p1, r1, g1, b1) in pairwise(stops):
        if t <= p1:
            span = (p1 - p0) or 1.0
            u = (t - p0) / span
            return _hex(r0 + (r1 - r0) * u, g0 + (g1 - g0) * u, b0 + (b1 - b0) * u)
    _, r, g, b = stops[-1]
    return _hex(r, g, b)


def severity_stops() -> list[list[float]]:
    """The severity ramp's stop table, for a page that redraws client-side.

    The conflict map recolours focused cells in JavaScript. Re-expressing the
    curve there by hand is how the focused and unfocused views drift apart, so
    the table is handed over as data instead.

    Returns:
        Ascending ``[position, red, green, blue]`` rows, channels 0-1.
    """
    return [list(stop) for stop in _SEVERITY_STOPS]


#: Counts below this each get a band of their own. One, two and three mods in a
#: cell are genuinely different situations and the difference is what people
#: look at the map to see; a continuous ramp normalised against a worst case of
#: forty rendered all three as the same dark blue.
COVERAGE_SINGLE_MAX: Final[int] = 5

#: Above that, counts are grouped in fives (6-10, 11-15, ...). Past five mods
#: the exact number stops changing what you would do about it, so the extra
#: resolution buys nothing and costs discriminability everywhere else.
COVERAGE_GROUP: Final[int] = 5

#: Ceiling on how many bands a map can have. Beyond this the top band becomes
#: open-ended ("76+"). A ramp is only readable while its steps are telling
#: apart -- forty bands over seven colour stops is a gradient again, with a
#: legend nobody can use.
COVERAGE_MAX_BANDS: Final[int] = 16


def coverage_bands(worst: int) -> list[tuple[int, int | None]]:
    """Build the mods-per-cell bands for a map.

    Args:
        worst: The highest mods-per-cell count on the map.

    Returns:
        Ascending ``(low, high)`` pairs covering 1..``worst``. ``high`` is
        ``None`` on the last band when it is open-ended, which happens only
        when the exact bands would exceed :data:`COVERAGE_MAX_BANDS`.
    """
    if worst < 1:
        return []
    singles: list[tuple[int, int | None]] = [
        (n, n) for n in range(1, min(worst, COVERAGE_SINGLE_MAX) + 1)
    ]
    bands = list(singles)
    low = COVERAGE_SINGLE_MAX + 1
    while low <= worst:
        high = low + COVERAGE_GROUP - 1
        if len(bands) + 1 >= COVERAGE_MAX_BANDS and high < worst:
            bands.append((low, None))
            return bands
        bands.append((low, min(high, worst)))
        low = high + 1
    return bands


def coverage_band_index(count: int, worst: int) -> int:
    """Find which band a count falls in.

    Args:
        count: How many mods touch the cell.
        worst: The highest count on the map.

    Returns:
        The band's index, clamped into range for counts outside 1..``worst``
        (a stale ``worst`` must colour something plausible, not raise).
    """
    bands = coverage_bands(worst)
    if not bands:
        return 0
    for index, (low, high) in enumerate(bands):
        if count >= low and (high is None or count <= high):
            return index
    return 0 if count < 1 else len(bands) - 1


def coverage_heat(count: int, worst: int) -> str:
    """Map a mods-per-cell count to its band's colour.

    Banded rather than continuous: the counts people act on are small and
    crowded at the bottom of the range, so a linear ramp against the worst cell
    on the map spent most of its colour on distinctions nobody needs.

    Args:
        count: How many mods touch this cell.
        worst: The highest count on the map. Coverage uses the true maximum
            rather than a percentile: unlike conflict counts, "the most-touched
            cell" is a reference point a user can find in the list, not an
            outlier to be clamped away.

    Returns:
        A ``#rrggbb`` string.
    """
    if count <= 0:
        return NEUTRAL
    bands = coverage_bands(worst)
    if len(bands) <= 1:
        return _hex(*_COVERAGE_STOPS[0][1:])
    return _ramp(_clamp(coverage_band_index(count, worst) / (len(bands) - 1)), _COVERAGE_STOPS)


def coverage_legend_stops(worst: int) -> list[tuple[str, str, bool]]:
    """Build ``(label, colour, needs_light_text)`` rows for a legend.

    One row per band, generated from the same banding the map draws with, so a
    legend can never drift from the colours beside it -- and, now that the map
    is banded, the legend is the map's key rather than a sample of a gradient.

    Args:
        worst: The highest mods-per-cell count on the map.

    Returns:
        Ascending rows. ``needs_light_text`` is a luminance test, so the label
        stays readable on both ends of the ramp.
    """
    rows: list[tuple[str, str, bool]] = []
    for low, high in coverage_bands(worst):
        colour = coverage_heat(low, worst)
        red, green, blue = (int(colour[i : i + 2], 16) / 255 for i in (1, 3, 5))
        luminance = 0.299 * red + 0.587 * green + 0.114 * blue
        if high is None:
            label = f"{low}+"
        elif high == low:
            label = str(low)
        else:
            label = f"{low}-{high}"
        rows.append((label, colour, luminance < 0.55))
    return rows


def saturation_point(counts: Sequence[int], percentile: float = 0.95) -> int:
    """Choose the count at which the severity ramp should saturate.

    Using the maximum lets a single pathological cell -- a landscape record
    touched by forty plugins -- compress everything else into the green end.
    Using a high percentile keeps the ordinary range legible and simply clamps
    the outliers, which are already the reddest thing on the map.

    Args:
        counts: Every cell's conflict count.
        percentile: Fraction of cells that should fall below the saturation
            point.

    Returns:
        The saturating count, always at least 1.
    """
    if not counts:
        return 0
    ordered = sorted(counts)
    index = min(len(ordered) - 1, int(len(ordered) * percentile))
    return max(1, ordered[index])


def legend_stops(worst: int, steps: int = 6) -> list[tuple[int, str]]:
    """Build ``(count, colour)`` pairs for a severity legend.

    Args:
        worst: The highest count on the map.
        steps: How many swatches to produce.

    Returns:
        Ascending ``(count, colour)`` pairs. Empty when there is nothing to
        show, so the caller can omit the legend entirely.
    """
    if worst <= 0 or steps <= 0:
        return []
    out: list[tuple[int, str]] = []
    for index in range(steps):
        count = max(1, round(worst * (index + 1) / steps))
        pair = (count, severity(count, worst))
        if not out or out[-1][0] != count:
            out.append(pair)
    return out
