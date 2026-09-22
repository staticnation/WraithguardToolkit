"""Landscape texture-index remapping, ported from merge_to_master.

A port of ``merge_to_master``'s ``traits/remap_textures.rs`` (public domain,
Greatness7).

A land texture (``LTEX``) has an index, and an exterior's landscape paints its
ground by those indices. The indices are local to a plugin, so the same texture
can hold different indices in different plugins. When merging, a plugin's texture
indices are rewritten to match the target file's, and its landscapes' painted
grids are updated to follow.
"""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

from wraithguard.esp.records.landscapetexture import LandscapeTexture

if TYPE_CHECKING:
    from wraithguard.merge.model import PluginData

#: In a landscape's ``VTEX`` grid, index 0 means "no texture", so a stored index
#: is one more than the ``LTEX`` index it refers to.
_VTEX_BIAS = 1


def next_texture_index(plugin: PluginData) -> int | None:
    """The next free texture index in ``plugin``: highest ``LTEX`` index plus one.

    Returns ``None`` when the plugin defines no land textures.
    """
    highest: int | None = None
    for obj in plugin.objects.values():
        if isinstance(obj, LandscapeTexture):
            highest = obj.index if highest is None else max(highest, obj.index)
    return highest + 1 if highest is not None else None


def get_index_remap(plugin: PluginData, master: PluginData) -> dict[int, int] | None:
    """Rewrite ``plugin``'s texture indices to agree with ``master``.

    Mutates each of the plugin's ``LTEX`` records to its new index and returns the
    grid-value remap (biased by one, since 0 means "no texture"). Returns ``None``
    when the master defines no textures, matching the source tool.

    Args:
        plugin: The plugin being remapped, whose ``LTEX`` indices are updated.
        master: The target file, whose indices are authoritative.

    Returns:
        A ``{old_grid_value: new_grid_value}`` map, or ``None``.
    """
    counter = next_texture_index(master)
    if counter is None:
        return None

    remap: dict[int, int] = {}
    for key, obj in plugin.objects.items():
        if not isinstance(obj, LandscapeTexture):
            continue
        old_index = obj.index
        master_obj = master.objects.get(key)
        if isinstance(master_obj, LandscapeTexture):
            new_index = master_obj.index
        else:
            new_index = counter
            counter += 1
        if old_index == new_index:
            continue
        obj.index = new_index
        if old_index >= 0xFFFF or new_index >= 0xFFFF:
            raise ValueError("Landscape texture index does not fit in the grid's u16.")
        remap[old_index + _VTEX_BIAS] = new_index + _VTEX_BIAS
    return remap


def apply_index_remap(plugin: PluginData, remap: dict[int, int]) -> None:
    """Rewrite the painted texture grid of every exterior landscape."""
    for exterior in plugin.cells.exteriors.values():
        landscape = exterior.landscape
        if landscape is None:
            continue
        raw = landscape.texture_indices
        count = len(raw) // 2
        values = list(struct.unpack(f"<{count}H", raw))
        changed = False
        for i, value in enumerate(values):
            new_value = remap.get(value)
            if new_value is not None:
                values[i] = new_value
                changed = True
        if changed:
            landscape.texture_indices = struct.pack(f"<{count}H", *values)


def remap_textures(plugin: PluginData, master: PluginData) -> None:
    """Remap ``plugin``'s land-texture indices (and painted grids) to match ``master``."""
    remap = get_index_remap(plugin, master)
    if remap:
        apply_index_remap(plugin, remap)
