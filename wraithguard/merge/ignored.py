"""Ignored-object handling, ported from merge_to_master.

A port of ``merge_to_master``'s ``traits/remove_ignored.rs`` and the
``set_all_ignored`` helper (public domain, Greatness7).

When a load order is merged, masters other than the merge target are loaded only
for the context they provide (so references resolve and dialogue orders line up).
Those context objects are flagged IGNORED while loaded and stripped afterwards,
so the merged file carries only what the plugins actually contributed.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from wraithguard.merge.model import is_ignored, set_ignored

if TYPE_CHECKING:
    from wraithguard.merge.model import Exterior, Interior, PluginData


def set_all_ignored(plugin: PluginData, ignored: bool) -> None:
    """Set or clear the IGNORED flag on every object in ``plugin``."""
    for obj in plugin.objects.values():
        set_ignored(obj, ignored)
    for interior in plugin.cells.interiors.values():
        if interior.cell is not None:
            set_ignored(interior.cell, ignored)
        if interior.pathgrid is not None:
            set_ignored(interior.pathgrid, ignored)
    for exterior in plugin.cells.exteriors.values():
        if exterior.cell is not None:
            set_ignored(exterior.cell, ignored)
        if exterior.landscape is not None:
            set_ignored(exterior.landscape, ignored)
        if exterior.pathgrid is not None:
            set_ignored(exterior.pathgrid, ignored)
    for group in plugin.dialogues.values():
        set_ignored(group.dialogue, ignored)
        for info in group.infos:
            set_ignored(info, ignored)


def _strip_ignored_exterior(exterior: Exterior) -> None:
    """Drop an exterior's ignored components in place."""
    if exterior.cell is not None and is_ignored(exterior.cell):
        exterior.cell = None
    if exterior.landscape is not None and is_ignored(exterior.landscape):
        exterior.landscape = None
    if exterior.pathgrid is not None and is_ignored(exterior.pathgrid):
        exterior.pathgrid = None


def _strip_ignored_interior(interior: Interior) -> None:
    """Drop an interior's ignored components in place."""
    if interior.cell is not None and is_ignored(interior.cell):
        interior.cell = None
    if interior.pathgrid is not None and is_ignored(interior.pathgrid):
        interior.pathgrid = None


def remove_ignored(plugin: PluginData) -> None:
    """Remove every object marked IGNORED, dropping cells and topics left empty."""
    plugin.objects = {key: obj for key, obj in plugin.objects.items() if not is_ignored(obj)}

    kept_ext = {}
    for coords, exterior in plugin.cells.exteriors.items():
        _strip_ignored_exterior(exterior)
        if exterior.count_objects() != 0:
            kept_ext[coords] = exterior
    plugin.cells.exteriors = kept_ext

    kept_int = {}
    for name, interior in plugin.cells.interiors.items():
        _strip_ignored_interior(interior)
        if interior.count_objects() != 0:
            kept_int[name] = interior
    plugin.cells.interiors = kept_int

    kept_dial = {}
    for topic, group in plugin.dialogues.items():
        group.infos = [info for info in group.infos if not is_ignored(info)]
        if not is_ignored(group.dialogue):
            kept_dial[topic] = group
    plugin.dialogues = kept_dial
