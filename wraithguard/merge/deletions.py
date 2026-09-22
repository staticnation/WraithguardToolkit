"""Deleted-object handling, ported from merge_to_master.

A port of ``merge_to_master``'s ``traits/remove_deleted.rs`` (public domain,
Greatness7).

Dropping a deleted object is easy; the hard part is that other objects may still
*point* at it -- a door's open sound, an NPC's class, a container's inventory, a
cell's placed references. Left dangling, those point at nothing. This module
records what was deleted (by id, and by which object types share that id) and
then walks every surviving object clearing the fields that referred to something
gone.
"""

from __future__ import annotations

from collections import defaultdict
from typing import TYPE_CHECKING

from wraithguard.esp.records._ai import (
    AiActivatePackage,
    AiEscortPackage,
    AiFollowPackage,
    TravelDestination,
)
from wraithguard.esp.records.activator import Activator
from wraithguard.esp.records.alchemy import Alchemy
from wraithguard.esp.records.apparatus import Apparatus
from wraithguard.esp.records.armor import Armor
from wraithguard.esp.records.birthsign import Birthsign
from wraithguard.esp.records.book import Book
from wraithguard.esp.records.clothing import Clothing
from wraithguard.esp.records.container import Container
from wraithguard.esp.records.creature import Creature
from wraithguard.esp.records.door import Door
from wraithguard.esp.records.ingredient import Ingredient
from wraithguard.esp.records.leveledcreature import LeveledCreature
from wraithguard.esp.records.leveleditem import LeveledItem
from wraithguard.esp.records.light import Light
from wraithguard.esp.records.lockpick import Lockpick
from wraithguard.esp.records.magiceffect import MagicEffect
from wraithguard.esp.records.miscitem import MiscItem
from wraithguard.esp.records.npc import Npc
from wraithguard.esp.records.probe import Probe
from wraithguard.esp.records.race import Race
from wraithguard.esp.records.region import Region
from wraithguard.esp.records.repairitem import RepairItem
from wraithguard.esp.records.soundgen import SoundGen
from wraithguard.esp.records.startscript import StartScript
from wraithguard.esp.records.weapon import Weapon
from wraithguard.merge.model import editor_id, is_deleted

if TYPE_CHECKING:
    from wraithguard.esp.record import Record
    from wraithguard.esp.records.cell import Cell
    from wraithguard.merge.model import PluginData

#: Object-type tags grouped the way references to them are cleaned. ``PHYSICAL``
#: is the shared id-space of everything a cell reference (or an inventory entry)
#: can name.
PHYSICAL: frozenset[bytes] = frozenset(
    {
        b"ACTI",
        b"ALCH",
        b"APPA",
        b"ARMO",
        b"BODY",
        b"BOOK",
        b"CLOT",
        b"CONT",
        b"CREA",
        b"DOOR",
        b"INGR",
        b"LEVC",
        b"LEVI",
        b"LIGH",
        b"LOCK",
        b"MISC",
        b"NPC_",
        b"PROB",
        b"REPA",
        b"STAT",
        b"WEAP",
    }
)
SPELL: frozenset[bytes] = frozenset({b"SPEL"})
SOUND: frozenset[bytes] = frozenset({b"SOUN"})
SCRIPT: frozenset[bytes] = frozenset({b"SCPT"})
ENCHANTING: frozenset[bytes] = frozenset({b"ENCH"})
CLASS: frozenset[bytes] = frozenset({b"CLAS"})
FACTION: frozenset[bytes] = frozenset({b"FACT"})
REGION: frozenset[bytes] = frozenset({b"REGN"})
CELL: frozenset[bytes] = frozenset({b"CELL"})

#: Records whose only reference to another object is a result ``script``.
_SCRIPT_ONLY = (RepairItem, Activator, Apparatus, Lockpick, Probe, Ingredient, Alchemy)

Deletions = dict[str, set[bytes]]


def _hits(deletions: Deletions, ident: str, tags: frozenset[bytes]) -> bool:
    """Whether ``ident`` names something deleted of one of ``tags``."""
    if not ident:
        return False
    return bool(deletions.get(ident.lower(), set()) & tags)


def _clean_str(record: object, attr: str, tags: frozenset[bytes], deletions: Deletions) -> None:
    """Clear a record's string field if it names a deleted object of ``tags``."""
    value = getattr(record, attr)
    if value and _hits(deletions, value, tags):
        setattr(record, attr, "")


def _clean_str_list(values: list[str], tags: frozenset[bytes], deletions: Deletions) -> list[str]:
    """Return ``values`` without ids that name a deleted object of ``tags``."""
    return [v for v in values if not _hits(deletions, v, tags)]


def _clean_pair_list(
    values: list[tuple], index: int, tags: frozenset[bytes], deletions: Deletions
) -> list[tuple]:
    """Return pairs whose ``index`` element does not name a deleted object of ``tags``."""
    return [pair for pair in values if not _hits(deletions, pair[index], tags)]


def _clean_ai_packages(packages: list, deletions: Deletions) -> None:
    """Clear AI-package targets and cells that name deleted objects."""
    for package in packages:
        if isinstance(package, (AiEscortPackage, AiFollowPackage)):
            if _hits(deletions, package.target, PHYSICAL):
                package.target = ""
            if _hits(deletions, package.cell, CELL):
                package.cell = ""
        elif isinstance(package, AiActivatePackage) and _hits(deletions, package.target, PHYSICAL):
            package.target = ""


def _clean_travel(destinations: list[TravelDestination], deletions: Deletions) -> None:
    """Clear travel destinations whose cell was deleted."""
    for destination in destinations:
        if _hits(deletions, destination.cell, CELL):
            destination.cell = ""


def _clean_biped(biped_objects: list, deletions: Deletions) -> None:
    """Clear biped body-part slots that name deleted bodyparts."""
    for biped in biped_objects:
        if _hits(deletions, biped.male_bodypart, PHYSICAL):
            biped.male_bodypart = ""
        if _hits(deletions, biped.female_bodypart, PHYSICAL):
            biped.female_bodypart = ""


def clean_object(obj: Record, deletions: Deletions) -> None:
    """Clear every field of ``obj`` that refers to a deleted object.

    Args:
        obj: A surviving object.
        deletions: Deleted ids mapped to the object-type tags deleted under them.
    """
    if isinstance(obj, _SCRIPT_ONLY):
        _clean_str(obj, "script", SCRIPT, deletions)
    elif isinstance(obj, Door):
        _clean_str(obj, "script", SCRIPT, deletions)
        _clean_str(obj, "open_sound", SOUND, deletions)
        _clean_str(obj, "close_sound", SOUND, deletions)
    elif isinstance(obj, MiscItem):
        _clean_str(obj, "script", SCRIPT, deletions)
    elif isinstance(obj, (Weapon, Book)):
        _clean_str(obj, "script", SCRIPT, deletions)
        _clean_str(obj, "enchanting", ENCHANTING, deletions)
    elif isinstance(obj, (Armor, Clothing)):
        _clean_str(obj, "script", SCRIPT, deletions)
        _clean_str(obj, "enchanting", ENCHANTING, deletions)
        _clean_biped(obj.biped_objects, deletions)
    elif isinstance(obj, Light):
        _clean_str(obj, "script", SCRIPT, deletions)
        _clean_str(obj, "sound", SOUND, deletions)
    elif isinstance(obj, Container):
        _clean_str(obj, "script", SCRIPT, deletions)
        obj.inventory = _clean_pair_list(obj.inventory, 1, PHYSICAL, deletions)
    elif isinstance(obj, Creature):
        _clean_str(obj, "script", SCRIPT, deletions)
        obj.inventory = _clean_pair_list(obj.inventory, 1, PHYSICAL, deletions)
        obj.spells = _clean_str_list(obj.spells, SPELL, deletions)
        _clean_ai_packages(obj.ai_packages, deletions)
        _clean_travel(obj.travel_destinations, deletions)
    elif isinstance(obj, Npc):
        _clean_str(obj, "script", SCRIPT, deletions)
        obj.inventory = _clean_pair_list(obj.inventory, 1, PHYSICAL, deletions)
        obj.spells = _clean_str_list(obj.spells, SPELL, deletions)
        _clean_ai_packages(obj.ai_packages, deletions)
        _clean_travel(obj.travel_destinations, deletions)
        _clean_str(obj, "class_", CLASS, deletions)
        _clean_str(obj, "faction", FACTION, deletions)
        _clean_str(obj, "head", PHYSICAL, deletions)
        _clean_str(obj, "hair", PHYSICAL, deletions)
        # Note: race is deliberately not cleaned -- doing so crashes the TESCS.
    elif isinstance(obj, (Race, Birthsign)):
        obj.spells = _clean_str_list(obj.spells, SPELL, deletions)
    elif isinstance(obj, StartScript):
        _clean_str(obj, "script", SCRIPT, deletions)
    elif isinstance(obj, SoundGen):
        _clean_str(obj, "creature", PHYSICAL, deletions)
        _clean_str(obj, "sound", SOUND, deletions)
    elif isinstance(obj, MagicEffect):
        for attr in ("bolt_sound", "cast_sound", "hit_sound", "area_sound"):
            _clean_str(obj, attr, SOUND, deletions)
        for attr in ("cast_visual", "bolt_visual", "hit_visual", "area_visual"):
            _clean_str(obj, attr, PHYSICAL, deletions)
    elif isinstance(obj, Region):
        _clean_str(obj, "sleep_creature", PHYSICAL, deletions)
        obj.sounds = _clean_pair_list(obj.sounds, 0, SOUND, deletions)
    elif isinstance(obj, LeveledItem):
        obj.items = _clean_pair_list(obj.items, 0, PHYSICAL, deletions)
    elif isinstance(obj, LeveledCreature):
        obj.creatures = _clean_pair_list(obj.creatures, 0, PHYSICAL, deletions)


def clean_cell(cell: Cell, deletions: Deletions) -> None:
    """Clear a cell's deleted region and drop references to deleted objects."""
    if cell.region and _hits(deletions, cell.region, REGION):
        cell.region = None

    kept = []
    for reference in cell.references:
        if reference.mast_index != 0:
            kept.append(reference)  # not local to this plugin
        elif reference.deleted:
            continue  # explicitly deleted
        elif _hits(deletions, reference.id, PHYSICAL):
            continue  # implicitly deleted via its object
        else:
            kept.append(reference)
    cell.references = kept


def _remove_deleted_cells(plugin: PluginData) -> None:
    """Drop deleted cell components, then any cell left without its ``CELL``."""
    kept_ext = {}
    for coords, exterior in plugin.cells.exteriors.items():
        if exterior.cell is not None and is_deleted(exterior.cell):
            exterior.cell = None
        if exterior.pathgrid is not None and is_deleted(exterior.pathgrid):
            exterior.pathgrid = None
        if exterior.landscape is not None and is_deleted(exterior.landscape):
            exterior.landscape = None
        if exterior.cell is not None and exterior.count_objects() != 0:
            kept_ext[coords] = exterior
    plugin.cells.exteriors = kept_ext

    kept_int = {}
    for name, interior in plugin.cells.interiors.items():
        if interior.cell is not None and is_deleted(interior.cell):
            interior.cell = None
        if interior.pathgrid is not None and is_deleted(interior.pathgrid):
            interior.pathgrid = None
        if interior.cell is not None and interior.count_objects() != 0:
            kept_int[name] = interior
    plugin.cells.interiors = kept_int


def _remove_deleted_dialogue(plugin: PluginData) -> None:
    """Drop deleted topics and responses, mending the linked-list ends."""
    kept = {}
    for topic, group in plugin.dialogues.items():
        if is_deleted(group.dialogue):
            continue
        if any(is_deleted(info) for info in group.infos):
            front_deleted = bool(group.infos) and is_deleted(group.infos[0])
            back_deleted = bool(group.infos) and is_deleted(group.infos[-1])
            group.infos = [info for info in group.infos if not is_deleted(info)]
            if group.infos:
                group.repair_links()
                if front_deleted:
                    group.infos[0].prev_id = ""
                if back_deleted:
                    group.infos[-1].next_id = ""
        kept[topic] = group
    plugin.dialogues = kept


def remove_deleted(plugin: PluginData) -> None:
    """Remove all deleted objects and clean every reference left dangling by them."""
    deletions: Deletions = defaultdict(set)

    # Extract deleted ordinary objects, recording their id and type.
    for key in list(plugin.objects):
        obj = plugin.objects[key]
        if is_deleted(obj):
            del plugin.objects[key]
            deletions[editor_id(obj).lower()].add(obj.TAG)

    # Deleted interior cells: recorded by name so travel/AI cells can be cleaned.
    for name in list(plugin.cells.interiors):
        cell = plugin.cells.interiors[name].cell
        if cell is not None and is_deleted(cell):
            deletions[name].add(b"CELL")

    # Clean every surviving object and cell. This runs even with no deletions,
    # because it also strips references explicitly marked deleted in a cell.
    for obj in plugin.objects.values():
        clean_object(obj, deletions)
    for cell in plugin.cells.iter_cells():
        clean_cell(cell, deletions)

    _remove_deleted_cells(plugin)
    _remove_deleted_dialogue(plugin)
