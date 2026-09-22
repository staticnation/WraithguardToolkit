"""Merging one plugin's buckets into another, ported from merge_to_master.

A port of ``merge_to_master``'s ``traits/merge_objects.rs`` (public domain,
Greatness7).

The rule is last-plugin-wins, with three exceptions that need real merging:

* **Cells** keep the latest header fields but *union* their references, keyed by
  the reference's ``(master index, object index)`` -- so two mods editing
  different objects in the same cell both survive, while two edits of the same
  placement resolve to the later one.
* **Interiors/exteriors** merge their cell and keep the latest landscape and path
  grid.
* **Dialogue groups** keep the latest topic and splice in the incoming responses
  in linked-list order, then repair the ``prev``/``next`` links.

Everything else is replaced wholesale by the later plugin's version.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from wraithguard.esp.records.cell import Cell
    from wraithguard.merge.dialogue import DialogueGroup
    from wraithguard.merge.model import Exterior, Interior, PluginData


def merge_cell_into(src: Cell, dst: Cell) -> None:
    """Merge cell ``src`` into ``dst``: latest header fields, unioned references.

    Args:
        src: The later cell (wins on scalar fields).
        dst: The cell being merged into, mutated in place.
    """
    dst.flags = src.flags
    dst.name = src.name
    dst.data = src.data
    if src.region is not None:
        dst.region = src.region
    if src.map_color is not None:
        dst.map_color = src.map_color
    if src.water_height is not None:
        dst.water_height = src.water_height
    if src.atmosphere_data is not None:
        dst.atmosphere_data = src.atmosphere_data

    # Union references by (master index, object index); the later one wins.
    merged = {(r.mast_index, r.refr_index): r for r in dst.references}
    for reference in src.references:
        merged[(reference.mast_index, reference.refr_index)] = reference
    dst.references = list(merged.values())


def _merge_cell_slot(src_cell: Cell | None, dst: Interior | Exterior) -> None:
    """Merge an optional source cell into a container's cell slot."""
    if dst.cell is not None:
        if src_cell is not None:
            merge_cell_into(src_cell, dst.cell)
    else:
        dst.cell = src_cell


def merge_interior_into(src: Interior, dst: Interior) -> None:
    """Merge interior ``src`` into ``dst``."""
    _merge_cell_slot(src.cell, dst)
    if src.pathgrid is not None:
        dst.pathgrid = src.pathgrid


def merge_exterior_into(src: Exterior, dst: Exterior) -> None:
    """Merge exterior ``src`` into ``dst``."""
    _merge_cell_slot(src.cell, dst)
    if src.landscape is not None:
        dst.landscape = src.landscape
    if src.pathgrid is not None:
        dst.pathgrid = src.pathgrid


def merge_dialogue_group_into(src: DialogueGroup, dst: DialogueGroup) -> None:
    """Merge dialogue group ``src`` into ``dst``: latest topic, spliced responses."""
    dst.dialogue = src.dialogue
    dst.merge_infos(src.infos)
    dst.repair_links()


def merge_plugin_into(src: PluginData, dst: PluginData) -> None:
    """Merge every bucket of ``src`` into ``dst`` (``src`` is consumed).

    Args:
        src: The later plugin's data; its records are moved into ``dst``.
        dst: The accumulator, mutated in place.
    """
    dst.header = src.header

    for key, obj in src.objects.items():
        dst.objects[key] = obj

    for coords, exterior in src.cells.exteriors.items():
        existing = dst.cells.exteriors.get(coords)
        if existing is not None:
            merge_exterior_into(exterior, existing)
        else:
            dst.cells.exteriors[coords] = exterior

    for name, interior in src.cells.interiors.items():
        existing_int = dst.cells.interiors.get(name)
        if existing_int is not None:
            merge_interior_into(interior, existing_int)
        else:
            dst.cells.interiors[name] = interior

    for topic, group in src.dialogues.items():
        existing_group = dst.dialogues.get(topic)
        if existing_group is not None:
            merge_dialogue_group_into(group, existing_group)
        else:
            dst.dialogues[topic] = group
