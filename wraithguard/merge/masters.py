"""Master-list remapping, ported from merge_to_master.

A port of ``merge_to_master``'s ``traits/remap_masters.rs`` and the ``Header``
extensions in ``traits/extensions.rs`` (public domain, Greatness7).

A reference names the object it edits by two indices: which master defined it
(``mast_index``, a position in *this plugin's own* master list) and the object's
index within that master. Two plugins that both edit the same object can store
different ``mast_index`` values for it, because their master lists differ. Merge
them naively and the edits point at different objects. Remapping rewrites each
plugin's indices to agree with the file they are being merged into.
"""

from __future__ import annotations

import copy
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from wraithguard.merge.model import PluginData

#: A master list entry: file name and file size.
Master = tuple[str, int]


def ensure_master_present(header_masters: list[Master], master_name: str, master_size: int) -> None:
    """Ensure ``master_name`` is the last entry in ``header_masters``.

    Args:
        header_masters: The plugin's master list, mutated in place.
        master_name: The merge target's file name.
        master_size: Its size on disk, for a new entry.

    Raises:
        ValueError: If the target is present but not last.
    """
    position = next(
        (i for i, (name, _) in enumerate(header_masters) if name.lower() == master_name.lower()),
        None,
    )
    if position is None:
        header_masters.append((master_name, master_size))
    elif position != len(header_masters) - 1:
        raise ValueError("Merge target must be the last master in the plugin's master list.")


def collect_masters(plugin_paths: Sequence[Path]) -> list[Master]:
    """The master list for a merged load order: every plugin, in load order."""
    return [(path.name, path.stat().st_size) for path in plugin_paths]


def build_master_remap(masters: Sequence[Master]) -> dict[str, int]:
    """Map each master's (lower-cased) name to its 1-based position."""
    return {name.lower(): index + 1 for index, (name, _) in enumerate(masters)}


def get_index_remap(
    plugin_masters: Sequence[Master],
    master_masters: Sequence[Master],
    target_master: str,
) -> tuple[list[Master] | None, list[int] | None]:
    """Work out the new master list and per-index remap for merging one plugin.

    Args:
        plugin_masters: The plugin's own master list.
        master_masters: The target file's master list.
        target_master: The target's file name (remapped to local references).

    Returns:
        ``(new_masters, index_remap)``. Each is ``None`` when unchanged.
        ``index_remap[i]`` is the new master index for the plugin's old index ``i``
        (index 0 stays reserved for local references).
    """
    new_masters: list[Master] = list(master_masters)
    index_remap: list[int] = [0]  # index 0 is reserved for local references

    for name, size in plugin_masters:
        if name.lower() == target_master.lower():
            index_remap.append(0)
            continue
        pos = next((i for i, (n, _) in enumerate(new_masters) if n.lower() == name.lower()), None)
        if pos is None:
            new_masters.append((name, size))
            index_remap.append(len(new_masters))
        else:
            index_remap.append(pos + 1)

    masters_changed = list(master_masters) != new_masters
    indices_changed = any(i != j for i, j in enumerate(index_remap))
    return (
        new_masters if masters_changed else None,
        index_remap if indices_changed else None,
    )


def next_reference_index(plugin: PluginData) -> int:
    """The next free local reference index: the highest local one plus one, or 1."""
    highest: int | None = None
    for cell in plugin.cells.iter_cells():
        for reference in cell.references:
            if reference.mast_index == 0:
                highest = (
                    reference.refr_index if highest is None else max(highest, reference.refr_index)
                )
    return highest + 1 if highest is not None else 1


def apply_index_remap(plugin: PluginData, index_remap: Sequence[int], start_index: int) -> None:
    """Rewrite every reference's master index, renumbering local ones from ``start_index``."""
    next_index = start_index
    for cell in plugin.cells.iter_cells():
        for reference in cell.references:
            if reference.mast_index == 0:
                reference.refr_index = next_index
                next_index += 1
            else:
                reference.mast_index = index_remap[reference.mast_index]


def remap_masters(plugin: PluginData, master: PluginData, master_name: str) -> None:
    """Remap ``plugin``'s references to be consistent with ``master``, and adopt its header.

    Args:
        plugin: The plugin being prepared for merge, mutated in place.
        master: The file it will be merged into.
        master_name: The target master's file name.
    """
    new_masters, index_remap = get_index_remap(
        plugin.header.masters, master.header.masters, master_name
    )

    # Adopt the master file's header (author/description/etc.).
    plugin.header = copy.deepcopy(master.header)
    if new_masters is not None:
        plugin.header.masters = new_masters

    if index_remap is not None:
        apply_index_remap(plugin, index_remap, next_reference_index(master))


def remap_load_order_masters(
    plugin: PluginData, plugin_index: int, master_remap: dict[str, int]
) -> None:
    """Remap one plugin's references into a merged-load-order index space.

    Local references (master index 0) come to point at this plugin itself (now a
    master of the merged file); each of the plugin's masters is remapped to its
    position in the merged master list.

    Args:
        plugin: The loaded plugin, mutated in place.
        plugin_index: The plugin's 0-based position in the load order.
        master_remap: Name (lower-cased) to 1-based merged master index.

    Raises:
        ValueError: If one of the plugin's masters is not in ``master_remap``.
    """
    local_remap: list[int] = [plugin_index + 1]  # index 0 -> this plugin
    for name, _ in plugin.header.masters:
        mapped = master_remap.get(name.lower())
        if mapped is None:
            raise ValueError(f"Master '{name}' not found in the merged load order.")
        local_remap.append(mapped)

    # A reference whose mast_index is out of range names a master the plugin's
    # own header doesn't declare -- a dirty/corrupt plugin. Drop it rather than
    # crash the whole merge, matching merge_to_master's tolerant filter_map.
    for cell in plugin.cells.iter_cells():
        kept = []
        for reference in cell.references:
            if not 0 <= reference.mast_index < len(local_remap):
                continue
            reference.mast_index = local_remap[reference.mast_index]
            kept.append(reference)
        cell.references = kept
