"""Bridging the names tes3conv uses to the names the format reference uses.

The conflict scanner works from tes3conv's JSON, which tags each record with a
friendly type name (``"Landscape"``, ``"LandscapeTexture"``, ``"Npc"``). The
schema is keyed by the four-character tags the binary format actually stores
(``LAND``, ``LTEX``, ``NPC``). Nothing in either file states the correspondence,
so it is written out here once, explicitly, rather than guessed at per call site
by string-mangling -- ``LandscapeTexture`` does not shorten to ``LTEX`` by any
rule a computer would find, and ``Header`` to ``TES3`` least of all.

The pairs are facts about a naming convention, taken from the JSON this tool
already reads and cross-checked against the MIT-licensed Rust tooling in the
project's reference folder. No code was copied.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Final

from mlox_subset.tes3fields.schema import RECORDS

if TYPE_CHECKING:
    from mlox_subset.tes3fields.schema_types import Record, Subrecord

#: tes3conv's ``"type"`` value -> the record tag the format reference documents.
TYPE_TO_TAG: Final[dict[str, str]] = {
    "Activator": "ACTI",
    "Alchemy": "ALCH",
    "Apparatus": "APPA",
    "Armor": "ARMO",
    "Birthsign": "BSGN",
    "Bodypart": "BODY",
    "Book": "BOOK",
    "Cell": "CELL",
    "Class": "CLAS",
    "Clothing": "CLOT",
    "Container": "CONT",
    "Creature": "CREA",
    "Dialogue": "DIAL",
    "DialogueInfo": "INFO",
    "Door": "DOOR",
    "Enchanting": "ENCH",
    "Faction": "FACT",
    "GameSetting": "GMST",
    "GlobalVariable": "GLOB",
    "Header": "TES3",
    "Ingredient": "INGR",
    "Landscape": "LAND",
    "LandscapeTexture": "LTEX",
    "LeveledCreature": "LEVC",
    "LeveledItem": "LEVI",
    "Light": "LIGH",
    "Lockpick": "LOCK",
    "MagicEffect": "MGEF",
    "MiscItem": "MISC",
    "Npc": "NPC",
    "PathGrid": "PGRD",
    "Probe": "PROB",
    "Race": "RACE",
    "Region": "REGN",
    "RepairItem": "REPA",
    "Script": "SCPT",
    "Skill": "SKIL",
    "Sound": "SOUN",
    "SoundGen": "SNDG",
    "Spell": "SPEL",
    "StartScript": "SSCR",
    "Static": "STAT",
    "Weapon": "WEAP",
}

#: Shared subpages whose subrecords several record types include. The reference
#: documents these once rather than repeating them, so a field lookup has to
#: fall through to them or it will not find, say, ``AI_W`` on a creature.
_SHARED: Final[tuple[str, ...]] = (
    "AI Package Fields",
    "Biped Object Fields",
    "ENAM Field",
    "List",
)


def record_for(type_name: str) -> Record | None:
    """Find the documented layout for a tes3conv record type.

    Args:
        type_name: tes3conv's ``"type"`` value, or a raw four-character tag.

    Returns:
        The record, or ``None`` when the type is not one the reference covers --
        which is a normal answer, not an error: tes3conv reads record types the
        format pages do not document.
    """
    tag = TYPE_TO_TAG.get(type_name, type_name.upper().rstrip("_"))
    return RECORDS.get(tag)


def subrecord_for(type_name: str, tag: str) -> Subrecord | None:
    """Find one subrecord's documentation.

    Falls back to the shared subpages, because a creature's ``AI_W`` is
    documented on the AI package page rather than on ``CREA``.

    Args:
        type_name: tes3conv's ``"type"`` value, or a raw record tag.
        tag: The four-character subrecord tag, e.g. ``"NPDT"``.

    Returns:
        The subrecord, or ``None`` if neither the record nor any shared page
        documents it.
    """
    wanted = tag.upper()
    record = record_for(type_name)
    if record is not None:
        found = record.by_name.get(wanted)
        if found is not None:
            return found
    for shared in _SHARED:
        page = RECORDS.get(shared)
        if page is not None:
            found = page.by_name.get(wanted)
            if found is not None:
                return found
    return None
