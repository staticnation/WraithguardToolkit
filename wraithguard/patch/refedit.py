"""Changed references written the way the engine merges them: a cell carrying only them.

How a plugin changes a placed object, as greatness7's merge_to_master implements the
engine's rule (``types/cells.rs``, ``traits/merge_objects.rs``, ``traits/remap_masters.rs``)
and as Morrowind and OpenMW load it:

- A cell's references are keyed ``(mast_index, refr_index)``. ``mast_index`` names the
  file that **created** the reference: ``0`` is the plugin itself, ``k`` the ``k``-th
  entry of *that plugin's own* master list (from 1). The same object is ``(2, 15)`` in a
  plugin whose second master created it and ``(3, 15)`` in one whose third master did.
- A later plugin's CELL record is **merged into** the cell, not swapped for it: its
  ``flags``, ``name`` and ``data`` replace the cell's, its region, map colour, water and
  ambient replace them when it has them, and its references are **added by key** -
  a key already there is overridden, every other reference is left as it was.
- So a plugin that moves one object carries a CELL record with that one reference, keyed
  by the creating file's position in *its* master list. Carrying the cell's whole
  reference list instead would re-assert every reference in it, undoing what any plugin
  loaded in between did to the others.
- A reference the plugin adds itself is ``(0, n)``, ``n`` above every ``(0, ...)`` it
  already has (merge_to_master's ``next_reference_index``: the highest plus one, from 1).
- Deleting a placed object is the reference again with its ``deleted`` flag; one moved to
  another exterior cell keeps its key and gains a ``moved_cell``.

This module builds such a CELL record for the patch: the winning cell's own fields (so
the merge changes nothing else about the cell), and each changed reference as the load
order currently has it, with the changes applied, keyed for the patch's master list.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Final

from wraithguard.patch.records import PatchError, master_names, record_key

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence

#: The fields of a reference a change may set (the tes3 crate's ``Reference``). Its key
#: (``mast_index``, ``refr_index``) and what it is (``id``) are not changes: changing
#: them makes another reference.
REF_FIELDS: Final[frozenset[str]] = frozenset(
    {
        "translation",
        "rotation",
        "scale",
        "deleted",
        "moved_cell",
        "owner",
        "owner_global",
        "owner_faction",
        "owner_faction_rank",
        "lock_level",
        "key",
        "trap",
        "soul",
        "charge_left",
        "health_left",
        "object_count",
        "blocked",
        "temporary",
        "destination",
    }
)


@dataclass(frozen=True, slots=True)
class RefEdit:
    """Changes to one placed object.

    Attributes:
        cell: The cell's key (:func:`.records.record_key`: an interior's name, or
            ``"(x, y)"`` for an exterior).
        origin: The plugin that created the reference (its ``mast_index`` resolved).
        refr_index: Its ``refr_index``.
        changes: Field -> new value (fields of :data:`REF_FIELDS`).
        plugins: The plugins with a CELL record for the cell (each is read to find
            the reference as the load order resolves it).
    """

    cell: str
    origin: str
    refr_index: int
    changes: Mapping[str, Any] = field(default_factory=dict)
    plugins: tuple[str, ...] = ()

    @property
    def ident(self) -> tuple[str, str, int]:
        """What it is a change to, for de-duplicating: ``(cell, origin, refr_index)``."""
        return (self.cell.lower(), self.origin.lower(), self.refr_index)


def _origin(plugin: str, masters: Sequence[str], mast_index: int) -> str | None:
    """The file a reference's ``mast_index`` names, read in the plugin it is in.

    Args:
        plugin: The plugin the reference is in.
        masters: That plugin's master list.
        mast_index: The index.

    Returns:
        The file's name, or None for an index the list does not reach.
    """
    if mast_index == 0:
        return plugin
    return masters[mast_index - 1] if 0 < mast_index <= len(masters) else None


def cell_versions(
    cell: str,
    records_by_plugin: Mapping[str, Sequence[Mapping[str, Any]]],
    load_order: Sequence[str],
) -> list[tuple[str, Mapping[str, Any]]]:
    """Every plugin's CELL record for one cell, in load order.

    Args:
        cell: The cell's key.
        records_by_plugin: The plugins' decoded records (header included).
        load_order: The load order.

    Returns:
        ``(plugin, cell record)``.
    """
    out = []
    for plugin in load_order:
        for rec in records_by_plugin.get(plugin) or []:
            if rec.get("type") == "Cell" and record_key(rec).lower() == cell.lower():  # engine keys
                out.append((plugin, rec))
                break
    return out


def winning_reference(
    versions: Sequence[tuple[str, Mapping[str, Any]]],
    masters_of: Mapping[str, Sequence[str]],
    origin: str,
    refr_index: int,
) -> tuple[str, dict[str, Any]] | None:
    """The reference ``(origin, refr_index)`` as the load order resolves it.

    Args:
        versions: The cell's CELL records, in load order (:func:`cell_versions`).
        masters_of: Each plugin's master list.
        origin: The creating plugin.
        refr_index: The reference's index.

    Returns:
        ``(plugin whose version wins, the reference)``, or None when no version of the
        cell has it.
    """
    found: tuple[str, dict[str, Any]] | None = None
    for plugin, rec in versions:
        masters = masters_of.get(plugin, ())
        for ref in rec.get("references") or []:
            if not isinstance(ref, dict) or ref.get("refr_index") != refr_index:
                continue
            name = _origin(plugin, masters, int(ref.get("mast_index", -1)))
            if name is not None and name.lower() == origin.lower():
                found = (plugin, dict(ref))
    return found


def next_new_index(records: Iterable[Mapping[str, Any]]) -> int:
    """The ``refr_index`` for a reference the patch adds itself.

    Args:
        records: The patch's records so far (an earlier build carried forward, too).

    Returns:
        One above every ``(0, n)`` reference in them; 1 when there are none.
    """
    top = 0
    for rec in records:
        for ref in rec.get("references") or [] if rec.get("type") == "Cell" else []:
            if isinstance(ref, dict) and ref.get("mast_index") == 0:
                top = max(top, int(ref.get("refr_index", 0)))
    return top + 1


def cell_patch_record(
    edits: Sequence[RefEdit],
    records_by_plugin: Mapping[str, Sequence[Mapping[str, Any]]],
    load_order: Sequence[str],
    patch_masters: Sequence[str],
) -> dict[str, Any]:
    """The CELL record that carries a cell's changed references, and only those.

    Args:
        edits: The changes, all to one cell (one per reference).
        records_by_plugin: The plugins' decoded records (headers included).
        load_order: The load order.
        patch_masters: The master list the patch declares.

    Returns:
        The record: the winning version's own fields, and the changed references keyed
        for ``patch_masters``.

    Raises:
        PatchError: Edits to more than one cell, a cell or reference the load order does
            not have, a creating plugin the patch does not declare, or a change to a
            field that is not a reference's to change.
    """
    if not edits:
        raise PatchError("no reference changes to write")
    cells = {e.cell.lower() for e in edits}
    if len(cells) != 1:
        raise PatchError("one CELL record carries one cell's references")
    versions = cell_versions(edits[0].cell, records_by_plugin, load_order)
    if not versions:
        raise PatchError(f"no plugin of the load order has the cell {edits[0].cell}")
    masters_of = {p: master_names(records_by_plugin.get(p) or []) for p, _ in versions}
    position = {name.lower(): i for i, name in enumerate(patch_masters, start=1)}
    record = copy.deepcopy({k: v for k, v in versions[-1][1].items() if k != "references"})
    refs: list[dict[str, Any]] = []
    seen: set[tuple[str, int]] = set()
    for e in edits:
        ident = (e.origin.lower(), e.refr_index)
        if ident in seen:
            raise PatchError(f"two sets of changes for reference {e.origin}:{e.refr_index}")
        seen.add(ident)
        bad = set(e.changes) - REF_FIELDS
        if bad:
            raise PatchError(f"{', '.join(sorted(bad))} cannot be changed on a reference")
        won = winning_reference(versions, masters_of, e.origin, e.refr_index)
        if won is None:
            raise PatchError(f"the cell {e.cell} has no reference {e.origin}:{e.refr_index}")
        at = position.get(e.origin.lower())
        if at is None:
            raise PatchError(
                f"{e.origin} created this reference but is not among the patch's masters, "
                "so the reference cannot be named"
            )
        ref = copy.deepcopy(won[1])
        ref.update(copy.deepcopy(dict(e.changes)))
        ref["mast_index"] = at
        ref["refr_index"] = e.refr_index
        refs.append(ref)
    record["references"] = refs
    return record


__all__ = [
    "REF_FIELDS",
    "RefEdit",
    "cell_patch_record",
    "cell_versions",
    "next_new_index",
    "winning_reference",
]
