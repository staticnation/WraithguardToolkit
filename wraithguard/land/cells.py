"""Merge the ``CELL`` records that accompany merged terrain.

A ``CELL`` record carries a cell's own properties -- its name, region, map
colour, water height, weather, and flags -- alongside the list of references
placed in it. Mods that reshape terrain frequently adjust these too: lowering
water to match a dug channel, renaming a cell, changing its region.

**References are never carried.** Merged Lands emits its ``CELL`` records with
``references: default()`` -- an empty list -- and this does the same. That is
what makes emitting them safe: a ``CELL`` record with no references does not
displace the objects, NPCs or scripts any mod placed there, because reference
lists merge by ``(mast_index, refr_index)`` rather than being replaced wholesale.
The merged plugin says "this cell's water sits here" and stays silent about
everything in it.

**Flags accumulate rather than overwrite.** Two mods that each set a different
bit both meant it, and taking only the later one silently discards the earlier
mod's intent. The record's own flags and its data flags are both unioned.

Ported from ``merge/cells.rs`` in Merged Lands (MIT, David Von Derau).

**One deliberate divergence, and it is a bug fix.** The original reads:

.. code-block:: rust

    if let Some(record) = new.region.as_ref() {
        new.region = Some(record.clone());
        is_modified = true;
    }

That is ``new`` on both sides -- the field is cloned from itself, so a later
plugin's region, map colour, water height and atmosphere never actually apply.
The same shape appears for all four fields. The evident intent is ``rhs``, and
that is what this implements: a later plugin's value wins where it supplies one.
Reproducing the typo faithfully would mean a merge tool that cannot merge four
of the seven fields it reads.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Sequence

_log: Final = logging.getLogger(__name__)

#: Exterior cell coordinates.
Coords = tuple[int, int]

#: What tes3conv calls the record.
CELL_TYPE: Final = "Cell"

#: Fields where a later plugin's value replaces an earlier one, when it has one.
#: These are single values rather than sets of bits, so there is nothing to
#: union -- one water height has to win.
_LAST_WINS: Final[tuple[str, ...]] = (
    "region",
    "map_color",
    "water_height",
    "atmosphere_data",
)


@dataclass(slots=True)
class MergedCellRecord:
    """One exterior cell's merged properties.

    Attributes:
        coords: Where it is.
        record: The merged record, with an empty reference list.
        editors: Plugins that changed it, in load order.
        modified: Whether any plugin after the first actually changed anything.
    """

    coords: Coords
    record: dict[str, Any]
    editors: list[str] = field(default_factory=list)
    modified: bool = False


def _grid_of(record: dict[str, Any]) -> Coords | None:
    """Read a cell's exterior coordinates.

    Args:
        record: A decoded ``Cell`` record.

    Returns:
        The coordinates, or ``None`` for an interior cell -- which has no grid
        position and no landscape, so nothing here applies to it.
    """
    data = record.get("data")
    if not isinstance(data, dict):
        return None
    grid = data.get("grid")
    if not isinstance(grid, (list, tuple)) or len(grid) != 2:
        return None
    flags = data.get("flags")
    if isinstance(flags, str) and "IS_INTERIOR" in flags:
        return None
    return (int(grid[0]), int(grid[1]))


def _union_flags(left: str, right: str) -> str:
    """Combine two flag strings, keeping every named bit either sets.

    Args:
        left: One flag string.
        right: The other.

    Returns:
        The union, in a stable order.
    """
    names = {part.strip() for part in f"{left} | {right}".split("|") if part.strip()}
    return " | ".join(sorted(names))


def merge_cell_into(target: MergedCellRecord, incoming: dict[str, Any], plugin: str) -> None:
    """Fold one plugin's version of a cell into the running result.

    Args:
        target: The merged record so far, modified in place.
        incoming: This plugin's version.
        plugin: The plugin's name.
    """
    current = target.record
    changed = False

    incoming_flags = incoming.get("flags")
    if isinstance(incoming_flags, str) and incoming_flags != current.get("flags"):
        current["flags"] = _union_flags(str(current.get("flags", "")), incoming_flags)
        changed = True

    incoming_data = incoming.get("data")
    current_data = current.get("data")
    if isinstance(incoming_data, dict) and isinstance(current_data, dict):
        left = str(current_data.get("flags", ""))
        right = str(incoming_data.get("flags", ""))
        if left != right:
            current_data["flags"] = _union_flags(left, right)
            changed = True

    identifier = incoming.get("id")
    if isinstance(identifier, str) and identifier and identifier != current.get("id"):
        current["id"] = identifier
        changed = True

    # See the module docstring: the original clones these from itself, so they
    # never update. Taking the incoming value is the evident intent.
    for name in _LAST_WINS:
        value = incoming.get(name)
        if value is not None and value != current.get(name):
            current[name] = value
            changed = True

    if changed:
        target.modified = True
    target.editors.append(plugin)


def merge_cells(
    sources: Sequence[tuple[str, list[dict[str, object]]]],
    skip: frozenset[str] = frozenset(),
) -> dict[Coords, MergedCellRecord]:
    """Merge every exterior ``CELL`` record across a load order.

    Args:
        sources: ``(plugin name, records)`` in load order, masters first.
        skip: Plugin names to ignore -- previously generated merges, which
            would otherwise be folded back into their own successor.

    Returns:
        One merged record per exterior cell any plugin touched.
    """
    merged: dict[Coords, MergedCellRecord] = {}

    for name, records in sources:
        if name in skip:
            continue
        for record in records:
            if record.get("type") != CELL_TYPE:
                continue
            coords = _grid_of(record)  # type: ignore[arg-type]
            if coords is None:
                continue

            existing = merged.get(coords)
            if existing is None:
                merged[coords] = MergedCellRecord(
                    coords=coords,
                    record=_without_references(record),  # type: ignore[arg-type]
                    editors=[name],
                )
            else:
                merge_cell_into(existing, record, name)  # type: ignore[arg-type]

    _log.info("merged %d exterior cell record(s)", len(merged))
    return merged


def _without_references(record: dict[str, Any]) -> dict[str, Any]:
    """Copy a cell record with an empty reference list.

    Carrying references would make the merged plugin responsible for every
    object in the cell -- exactly the thing it must not become.

    Args:
        record: A decoded ``Cell`` record.

    Returns:
        A shallow copy with ``references`` emptied and ``data`` copied, since
        its flags are merged in place.
    """
    copy = {key: value for key, value in record.items() if key != "references"}
    copy["references"] = []
    data = copy.get("data")
    if isinstance(data, dict):
        copy["data"] = dict(data)
    return copy


def cells_for(merged: dict[Coords, MergedCellRecord], wanted: set[Coords]) -> list[dict[str, Any]]:
    """Select the cell records that accompany a set of merged landscapes.

    A ``CELL`` record is only worth emitting for a cell whose terrain the
    merged plugin also carries. One on its own would take ownership of a cell's
    properties without contributing any of the terrain that motivated it.

    Args:
        merged: Every merged cell record.
        wanted: The cells whose landscape is being written.

    Returns:
        The records to emit, in coordinate order.
    """
    return [merged[coords].record for coords in sorted(wanted) if coords in merged]
