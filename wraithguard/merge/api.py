"""The merge entry points, ported from merge_to_master.

A port of ``merge_to_master``'s ``merge_plugins.rs`` (public domain, Greatness7).

:func:`merge_plugins` folds one plugin into its master. :func:`merge_load_order`
folds a whole load order into a single merged master. :class:`MergeOptions`
carries the post-merge passes: dropping deleted objects, applying moved
references, and removing duplicate references.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

from wraithguard.esp.enums import FileType
from wraithguard.esp.plugin import read_plugin, read_plugin_filtered
from wraithguard.esp.records.header import Header
from wraithguard.merge.combine import merge_plugin_into
from wraithguard.merge.deletions import remove_deleted
from wraithguard.merge.ignored import remove_ignored, set_all_ignored
from wraithguard.merge.masters import (
    build_master_remap,
    collect_masters,
    ensure_master_present,
    remap_load_order_masters,
    remap_masters,
)
from wraithguard.merge.model import PluginData, is_ignored
from wraithguard.merge.textures import remap_textures

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from wraithguard.esp.records.reference import Reference

#: How close two reference transforms must be to count as duplicates.
_MAX_ABS_DIFF = 1e-5

#: Record tags a partial master is read for: cells (existence) and dialogue
#: (ordering context). Everything else is irrelevant to a merge.
_PARTIAL_TAGS: frozenset[bytes] = frozenset({b"CELL", b"DIAL", b"INFO"})


@dataclass(frozen=True)
class MergeOptions:
    """The post-merge passes to run on the merged result."""

    remove_deleted: bool = False
    apply_moved_references: bool = False
    preserve_duplicate_references: bool = False

    def apply(self, merged: PluginData) -> None:
        """Run the enabled passes over ``merged`` in the source tool's order."""
        if self.remove_deleted:
            remove_deleted(merged)
        if self.apply_moved_references:
            apply_moved_references(merged)
        if not self.preserve_duplicate_references:
            remove_duplicate_references(merged)


def _load(path: Path) -> PluginData:
    """Read a plugin file fully into a :class:`PluginData`."""
    return PluginData.from_records(read_plugin(path.read_bytes()))


def _load_partial(path: Path) -> PluginData:
    """Read only the cells and dialogue of a plugin, cells trimmed to their headers.

    A partial master supplies cell existence and dialogue ordering; its
    references and other cell fields are dropped so they cannot pollute the merge.
    """
    from wraithguard.esp.records.cell import Cell

    records = read_plugin_filtered(path.read_bytes(), _PARTIAL_TAGS)
    for record in records:
        if isinstance(record, Cell):
            record.references = []
            record.region = None
            record.map_color = None
            record.water_height = None
            record.atmosphere_data = None
    return PluginData.from_records(records)


def _merge_masters(plugin: PluginData, master_path: Path, master_name: str) -> PluginData:
    """Build the merged master from ``plugin``'s master list.

    Only the merge target is loaded in full; the others are loaded partially and
    flagged ignored, present only for context.
    """
    merged = PluginData()
    header = Header()
    for name, _ in plugin.header.masters:
        path = master_path.with_name(name)
        if name.lower() == master_name.lower():
            master = _load(path)
            header = master.header
        else:
            master = _load_partial(path)
            set_all_ignored(master, True)
        merge_plugin_into(master, merged)
    merged.header = header
    return merged


def merge_plugins(plugin_path: Path, master_path: Path, options: MergeOptions) -> PluginData:
    """Merge the plugin at ``plugin_path`` into the master at ``master_path``.

    Args:
        plugin_path: The plugin to merge in.
        master_path: The master it is merged into (must be the plugin's last master).
        options: The post-merge passes to run.

    Returns:
        The merged master as a :class:`PluginData`.
    """
    plugin = _load(plugin_path)
    ensure_master_present(plugin.header.masters, master_path.name, master_path.stat().st_size)
    master_name = master_path.name

    master = _merge_masters(plugin, master_path, master_name)

    remap_masters(plugin, master, master_name)
    remap_textures(plugin, master)
    merge_plugin_into(plugin, master)

    options.apply(master)
    remove_ignored(master)
    return master


def merge_load_order(plugin_paths: Sequence[Path], options: MergeOptions) -> PluginData:
    """Merge a whole load order into one master, resolved as the game would see it.

    Args:
        plugin_paths: The plugins, in load order.
        options: The post-merge passes to run.

    Returns:
        The merged result as a :class:`PluginData` (an ``.esm``).
    """
    masters = collect_masters(plugin_paths)
    master_remap = build_master_remap(masters)

    merged = PluginData()
    for index, path in enumerate(plugin_paths):
        plugin = _load(path)
        remap_load_order_masters(plugin, index, master_remap)
        remap_textures(plugin, merged)
        merge_plugin_into(plugin, merged)

    merged.header.file_type = FileType.Esm
    merged.header.masters = masters

    options.apply(merged)
    remove_ignored(merged)
    return merged


def _reference_transform(reference: Reference) -> tuple[float, ...]:
    """A reference's placement as one comparable tuple: translation, rotation, scale."""
    scale = reference.scale if reference.scale is not None else 1.0
    return (*reference.translation, *reference.rotation, scale)


def _transforms_equal(a: tuple[float, ...], b: tuple[float, ...]) -> bool:
    """Whether two placement tuples match within :data:`_MAX_ABS_DIFF`."""
    return all(abs(x - y) <= _MAX_ABS_DIFF for x, y in zip(a, b))


def remove_duplicate_references(merged: PluginData) -> None:
    """Drop references with the same id *and* transform, keeping the last of each."""
    for cell in merged.cells.iter_cells():
        groups: dict[str, list[tuple[int, tuple[float, ...]]]] = defaultdict(list)
        for i, reference in enumerate(cell.references):
            if not reference.deleted:
                groups[reference.id].append((i, _reference_transform(reference)))

        to_remove: set[int] = set()
        for group in groups.values():
            group.sort(key=lambda item: item[0])
            while group:
                _, transform = group.pop()  # keep the last (highest index)
                survivors = []
                for other_index, other_transform in group:
                    if _transforms_equal(transform, other_transform):
                        to_remove.add(other_index)
                    else:
                        survivors.append((other_index, other_transform))
                group[:] = survivors

        if to_remove:
            cell.references = [r for i, r in enumerate(cell.references) if i not in to_remove]


def apply_moved_references(merged: PluginData) -> None:
    """Move local ``moved_cell`` references into their destination exterior cells.

    Args:
        merged: The merged data, mutated in place.

    Raises:
        ValueError: If a moved reference names a destination cell that is absent.
    """
    moved: list[Reference] = []
    for exterior in merged.cells.exteriors.values():
        cell = exterior.cell
        if cell is None:
            continue
        remaining = []
        for reference in cell.references:
            if reference.mast_index == 0 and reference.moved_cell is not None:
                moved.append(reference)
            else:
                remaining.append(reference)
        cell.references = remaining

    for reference in moved:
        coords = reference.moved_cell
        if coords is None:  # pragma: no cover - only moved refs (coords set) reach here
            continue
        destination = merged.cells.exteriors.get(coords)
        if (
            destination is not None
            and destination.cell is not None
            and not is_ignored(destination.cell)
        ):
            reference.moved_cell = None
            destination.cell.references.append(reference)
        else:
            raise ValueError(f"Moved reference '{reference.id}' names an invalid cell {coords!r}.")
