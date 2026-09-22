"""Remove a master from a plugin's header and fix up its references.

OpenMW refuses to load a plugin whose header lists a master it can't find. Often
that master is no longer needed -- its content now comes from loose files or a
pluginless replacement (Better Bodies is the classic case) -- yet the stale
``MAST`` entry still blocks the load. Removing the entry is the fix, but a plugin
records each placed reference by *which master defined it*: the master's 1-based
position in the header, packed into the top byte of the reference's index. Drop a
master and every later master shifts down one, so those references must be
renumbered, and any reference that pointed *into* the removed master no longer
resolves and is dropped.

This mirrors Wrye Mash's master-remap (its ``FileRefs.remap`` /
``remapObject``), but works on our flat record list so unknown record types and
record order survive untouched -- only the header's master list and the cells'
reference indices change.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from wraithguard.esp.plugin import read_plugin, write_plugin
from wraithguard.esp.records.cell import Cell
from wraithguard.esp.records.header import Header

if TYPE_CHECKING:
    from collections.abc import Sequence

    from wraithguard.esp.record import Record


@dataclass
class RemoveMasterReport:
    """What removing a master did (or why it did nothing)."""

    master: str
    """The master name as it appeared in the header (or as requested if absent)."""
    removed: bool = False
    """Whether the master was present and removed."""
    references_dropped: int = 0
    """How many references pointed into the removed master and were dropped."""
    dropped_by_cell: dict[str, int] = field(default_factory=dict)
    """Per-cell count of dropped references, keyed by a human label."""
    references_remapped: int = 0
    """How many references had their master index shifted down."""
    remaining_masters: list[str] = field(default_factory=list)
    """The plugin's master list after removal."""


def _cell_label(cell: Cell) -> str:
    """A human label for a cell: its name, or its exterior grid coordinates."""
    if cell.name:
        return cell.name
    return f"({cell.data.grid[0]}, {cell.data.grid[1]})"


def _master_position(masters: Sequence[tuple[str, int]], master_name: str) -> int | None:
    """The 1-based header position of ``master_name`` (case-insensitively), or ``None``."""
    target = master_name.lower()
    for index, (name, _) in enumerate(masters):
        if name.lower() == target:
            return index + 1
    return None


def remove_master(records: list[Record], master_name: str) -> RemoveMasterReport:
    """Remove ``master_name`` from the plugin's header and renumber its references.

    Mutates ``records`` in place: the ``TES3`` header loses the master, and every
    cell reference is renumbered -- references into the removed master are dropped,
    references into later masters shift down one, local and earlier references are
    left alone.

    Args:
        records: The plugin's records, as returned by
            :func:`~wraithguard.esp.plugin.read_plugin`.
        master_name: The master file name to remove (matched case-insensitively).

    Returns:
        A :class:`RemoveMasterReport` describing what changed. When the master is
        not in the header, ``removed`` is ``False`` and nothing is touched.
    """
    header = next((r for r in records if isinstance(r, Header)), None)
    if header is None:
        return RemoveMasterReport(master=master_name, removed=False)

    position = _master_position(header.masters, master_name)
    if position is None:
        return RemoveMasterReport(
            master=master_name,
            removed=False,
            remaining_masters=[name for name, _ in header.masters],
        )

    matched_name = header.masters[position - 1][0]
    report = RemoveMasterReport(master=matched_name, removed=True)

    # Drop the master, then renumber references against the removed position.
    del header.masters[position - 1]

    for cell in records:
        if not isinstance(cell, Cell):
            continue
        kept = []
        dropped_here = 0
        for reference in cell.references:
            mast = reference.mast_index
            if mast == position:
                dropped_here += 1  # named the removed master: no longer resolves
                continue
            if mast > position:
                reference.mast_index = mast - 1  # a later master shifted down
                report.references_remapped += 1
            kept.append(reference)
        if dropped_here:
            cell.references = kept
            report.references_dropped += dropped_here
            report.dropped_by_cell[_cell_label(cell)] = dropped_here

    report.remaining_masters = [name for name, _ in header.masters]
    return report


@dataclass
class RenameMasterReport:
    """What renaming a master did (or why it did nothing)."""

    old_name: str
    """The master name as requested for renaming."""
    new_name: str
    """The name it was pointed at."""
    renamed: bool = False
    """Whether the old master was present and renamed."""
    remaining_masters: list[str] = field(default_factory=list)
    """The plugin's master list after the rename."""


def rename_master(
    records: list[Record], old_name: str, new_name: str, new_size: int
) -> RenameMasterReport:
    """Point a master entry at a differently-named file, keeping references intact.

    Only the header's ``MAST`` name and its recorded ``DATA`` size change; every
    reference keeps its master index, because the master stays at the same
    position in the list. This is the fix for a master that was simply renamed on
    disk (or whose content moved to an identically-structured file under a new
    name).

    Args:
        records: The plugin's records, as from
            :func:`~wraithguard.esp.plugin.read_plugin`.
        old_name: The master file name currently in the header (matched
            case-insensitively).
        new_name: The file name to point it at.
        new_size: The new master file's size on disk, recorded for the engine's
            master-size check.

    Returns:
        A :class:`RenameMasterReport`. When ``old_name`` is not in the header,
        ``renamed`` is ``False`` and nothing is touched.

    Raises:
        ValueError: If ``new_name`` is already a different master of the plugin
            (renaming onto it would create a duplicate master entry).
    """
    header = next((r for r in records if isinstance(r, Header)), None)
    if header is None:
        return RenameMasterReport(old_name=old_name, new_name=new_name, renamed=False)

    position = _master_position(header.masters, old_name)
    if position is None:
        return RenameMasterReport(
            old_name=old_name,
            new_name=new_name,
            renamed=False,
            remaining_masters=[name for name, _ in header.masters],
        )

    clash = _master_position(header.masters, new_name)
    if clash is not None and clash != position:
        raise ValueError(f"'{new_name}' is already a master of this plugin.")

    header.masters[position - 1] = (new_name, new_size)
    return RenameMasterReport(
        old_name=old_name,
        new_name=new_name,
        renamed=True,
        remaining_masters=[name for name, _ in header.masters],
    )


def remove_master_from_bytes(data: bytes, master_name: str) -> tuple[bytes, RemoveMasterReport]:
    """Read a plugin, remove a master, and hand back the new bytes plus a report.

    Args:
        data: The plugin file's bytes.
        master_name: The master file name to remove (matched case-insensitively).

    Returns:
        ``(new_bytes, report)``. When the master was not present, ``new_bytes`` is
        a faithful re-serialisation of the unchanged records and
        ``report.removed`` is ``False``.
    """
    records = read_plugin(data)
    report = remove_master(records, master_name)
    return write_plugin(records), report


def rename_master_in_bytes(
    data: bytes, old_name: str, new_name: str, new_size: int
) -> tuple[bytes, RenameMasterReport]:
    """Read a plugin, rename a master, and hand back the new bytes plus a report.

    Args:
        data: The plugin file's bytes.
        old_name: The master file name to rename (matched case-insensitively).
        new_name: The file name to point it at.
        new_size: The new master file's size on disk.

    Returns:
        ``(new_bytes, report)``. When ``old_name`` was not present, ``new_bytes``
        is a faithful re-serialisation of the unchanged records and
        ``report.renamed`` is ``False``.

    Raises:
        ValueError: If ``new_name`` is already a different master of the plugin.
    """
    records = read_plugin(data)
    report = rename_master(records, old_name, new_name, new_size)
    return write_plugin(records), report
