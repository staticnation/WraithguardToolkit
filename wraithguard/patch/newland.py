"""A ``LAND`` record of the patch's own: flat terrain for an exterior square.

The Construction Set makes land lazily - the first brush stroke on a square with none
allocates it, flat at height 0, which is the sea's level, so the square is flooded
until it is raised. This is that first allocation, made explicitly: every vertex of the
65 x 65 grid at one height, normals straight up, one texture over all 256 texture
cells. Sculpting and painting are the terrain tools' work, not this.

The record is built through :mod:`wraithguard.esp` (the form every record the patch
makes is in), so the plugin writer needs nothing of its own for it.
"""

from __future__ import annotations

import math
import struct
from typing import TYPE_CHECKING, Any, Final

from wraithguard.esp.flags import LandscapeFlags
from wraithguard.esp.json import record_to_json
from wraithguard.esp.records.landscape import Landscape

if TYPE_CHECKING:
    from collections.abc import Sequence

#: Vertices on a side of a cell's height grid (65 x 65), and the texture cells on a
#: side of its texture grid (16 x 16).
VERTICES: Final = 65
TEXTURE_CELLS: Final = 16

#: The world map (``WNAM``) is 9 x 9 signed bytes of 128 units each; vertex heights
#: past what a signed byte of those can say are held at its ends.
_WNAM_UNIT: Final = 128.0

#: Heights the editor accepts: the format's deltas are signed bytes of 8 units, so a
#: flat cell can sit anywhere a float can say, but nothing that high is a place in the game.
MAX_HEIGHT: Final = 30000.0


class LandError(ValueError):
    """Raised for land that cannot be made as asked."""


def flat_land(grid: Sequence[int], height: float = 0.0, texture_index: int = 0) -> dict[str, Any]:
    """A flat ``LAND`` record in tes3conv's form.

    Args:
        grid: The square, ``(x, y)``.
        height: Where every vertex sits, in game units (0 is the sea's level).
        texture_index: What every texture cell holds: 0 for the default ground, else
            a land texture's ``index`` plus one (as ``VTEX`` stores it).

    Returns:
        The record, ready to queue or write.

    Raises:
        LandError: For a height that is not a number or is out of range, or a texture
            index a ``VTEX`` entry cannot hold.
    """
    try:
        level = float(height)
    except (TypeError, ValueError) as exc:
        raise LandError("the height is a number") from exc
    if not math.isfinite(level) or abs(level) > MAX_HEIGHT:
        raise LandError(f"the height is within {MAX_HEIGHT:g} units of the sea's level")
    if isinstance(texture_index, bool) or not isinstance(texture_index, int):
        raise LandError("the texture index is a whole number")
    if not 0 <= texture_index <= 0xFFFF:
        raise LandError("the texture index does not fit a land cell's texture entry")
    vertices = VERTICES * VERTICES
    cells = TEXTURE_CELLS * TEXTURE_CELLS
    # A coarse map entry per 128 units, held to a signed byte: low ground reads as water.
    coarse = max(-128, min(127, round(level / _WNAM_UNIT)))
    record = Landscape(
        grid=(int(grid[0]), int(grid[1])),
        landscape_flags=(
            LandscapeFlags.USES_VERTEX_HEIGHTS_AND_NORMALS | LandscapeFlags.USES_TEXTURES
        ),
        # Straight up, as a signed byte triple per vertex (x, y, z = 0, 0, 127).
        vertex_normals=bytes((0, 0, 127)) * vertices,
        # The first vertex's height, then every step from the one before it: none.
        vertex_heights_offset=level,
        vertex_heights=bytes(vertices),
        world_map_data=struct.pack("b", coarse) * 81,
        texture_indices=struct.pack(f"<{cells}H", *([texture_index] * cells)),
    )
    return record_to_json(record)
