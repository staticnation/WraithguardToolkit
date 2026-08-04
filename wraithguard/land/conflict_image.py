"""Render a merge as PNG conflict maps, one per contributing plugin.

Merged Lands writes an image per plugin into a ``Conflicts`` folder: green
where the merge took that plugin's change without argument, yellow for a minor
conflict, red for a major one. Each image is drawn *relative to one plugin*, so
the question it answers is "how does the final land differ from what **this**
mod expected" -- which is the question a mod author actually has.

**Why an image rather than a number.** A count says 103 vertices were
contested. It cannot say they form a line across a road, or a blob in one
corner, or a scatter of single points. Terrain conflicts have shape, and shape
is what tells you whether a merge is fine or whether two mods are fighting over
something structural.

**Written with the toolkit's own encoder.** ``wraithguard.images.png`` writes
8-bit RGBA with the standard library and nothing else, so this adds no
dependency to a frozen build. The original uses the Rust ``image`` crate and
scales by 4; the same scale is used here, for the same reason -- a 65-pixel
image is too small to read, and nearest-neighbour keeps every vertex a crisp
block rather than blurring the very boundaries being inspected.

**A whole-landmass map too.** Per-cell images answer "what happened here"; a
single map over the world grid answers "where should I look first". One pixel
per cell, coloured by its worst conflict.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Final

from wraithguard.images.image import Image
from wraithguard.images.png import encode_png
from wraithguard.land.debug_colors import (
    MAJOR_COLOR,
    MINOR_COLOR,
    MODIFIED_COLOR,
)
from wraithguard.land.merge import ConflictParams, Severity, average_delta
from wraithguard.tes3fields.landscape import LAND_SIZE

if TYPE_CHECKING:
    from collections.abc import Mapping

    from wraithguard.land.diff import RelativeGrid

#: Exterior cell coordinates.
Coords = tuple[int, int]

#: Pixels per vertex. A 65x65 image is unreadable at native size; four gives a
#: 260-pixel tile that shows individual vertices.
SCALE: Final = 4

#: Terrain neither side changed.
UNTOUCHED: Final[tuple[int, int, int]] = (24, 26, 30)

#: Fully opaque.
_ALPHA: Final = 255


def _blit(pixels: bytearray, width: int, x: int, y: int, colour: tuple[int, int, int]) -> None:
    """Fill one scaled block.

    Args:
        pixels: The RGBA buffer, modified in place.
        width: Image width in pixels.
        x: Vertex column.
        y: Vertex row.
        colour: The block's colour.
    """
    red, green, blue = colour
    for row in range(SCALE):
        start = ((y * SCALE + row) * width + x * SCALE) * 4
        for column in range(SCALE):
            offset = start + column * 4
            pixels[offset] = red
            pixels[offset + 1] = green
            pixels[offset + 2] = blue
            pixels[offset + 3] = _ALPHA


def cell_conflict_image(
    merged: RelativeGrid | None,
    plugin: RelativeGrid | None,
    params: ConflictParams | None = None,
) -> bytes:
    """Draw one cell's conflicts, as seen from one plugin.

    **Rows are flipped.** Row 0 of a landscape grid is the *south* edge, while
    row 0 of an image is the top. Drawing them in the same order would print
    every map upside down against the world map and the cell map, which is the
    kind of error that is obvious in hindsight and invisible at the time.

    Args:
        merged: The merged terrain's differences.
        plugin: This plugin's differences.
        params: Severity thresholds, or ``None`` for the defaults.

    Returns:
        A PNG file.
    """
    side = LAND_SIZE * SCALE
    pixels = bytearray(side * side * 4)
    thresholds = params if params is not None else ConflictParams()

    for y in range(LAND_SIZE):
        for x in range(LAND_SIZE):
            colour = UNTOUCHED
            if plugin is not None and plugin.has_difference(x, y):
                colour = MODIFIED_COLOR
                if merged is not None and merged.has_difference(x, y):
                    colour = MINOR_COLOR
                    for one, two in zip(merged.deltas_at(x, y), plugin.deltas_at(x, y)):
                        if one == two:
                            continue
                        _, severity = average_delta(one, two, thresholds)
                        if severity is Severity.MAJOR:
                            colour = MAJOR_COLOR
                            break
            _blit(pixels, side, x, LAND_SIZE - 1 - y, colour)

    return encode_png(Image(width=side, height=side, pixels=bytes(pixels)))


def landmass_conflict_image(severity: Mapping[Coords, str]) -> bytes:
    """Draw the whole world grid, one pixel-block per cell.

    Args:
        severity: ``"major"``, ``"minor"`` or ``"clean"`` per cell.

    Returns:
        A PNG file.

    Raises:
        ValueError: If there are no cells to draw.
    """
    if not severity:
        raise ValueError("no cells to draw")

    xs = [coords[0] for coords in severity]
    ys = [coords[1] for coords in severity]
    left, right = min(xs), max(xs)
    bottom, top = min(ys), max(ys)
    width = (right - left + 1) * SCALE
    height = (top - bottom + 1) * SCALE
    pixels = bytearray(width * height * 4)

    palette = {"major": MAJOR_COLOR, "minor": MINOR_COLOR, "clean": MODIFIED_COLOR}
    for coords, level in severity.items():
        # Same flip as the per-cell map: +y is north, and north belongs at the
        # top of an image.
        column = coords[0] - left
        row = top - coords[1]
        _blit(pixels, width, column, row, palette.get(level, UNTOUCHED))

    return encode_png(Image(width=width, height=height, pixels=bytes(pixels)))
