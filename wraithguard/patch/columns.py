"""The Object Window's columns, record type by record type, as the Construction Set has them.

The viewer's engine lists a type's records with what every record has (id, name, model,
where it is defined, how often it is placed); the Construction Set shows each type with
its own columns - a lockpick's uses and quality, a weapon's damage, an NPC's race,
class and faction, a container's organic and respawn flags, a leveled list's entries.
This reads the winning version of every record of one type (the editor session's
reader, once per type) and gives those columns, so the page can show them beside the
engine's.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence

#: A column: its heading, how to read it off a record, how it shows (``text``, ``num``,
#: ``bool``), and its tooltip.
Column = tuple[str, "Callable[[Mapping[str, Any]], object]", str, str]


def _at(path: str) -> Callable[[Mapping[str, Any]], object]:
    """A reader for a dotted path."""

    def read(rec: Mapping[str, Any]) -> object:
        """The column's value for one record."""
        cur: object = rec
        for part in path.split("."):
            if not isinstance(cur, dict):
                return None
            cur = cur.get(part)
        return cur

    read.path = path  # type: ignore[attr-defined]  # the field it shows, edited in place
    return read


def _flag(path: str, name: str) -> Callable[[Mapping[str, Any]], object]:
    """A reader for one flag of a flags field (``A | B`` text, or a list)."""
    get = _at(path)

    def read(rec: Mapping[str, Any]) -> object:
        """The column's value for one record."""
        raw = get(rec)
        if isinstance(raw, (list, tuple)):
            return name in {str(x).upper() for x in raw}
        return name in {p.strip().upper() for p in str(raw or "").split("|")}

    return read


def _names(path: str, limit: int = 4) -> Callable[[Mapping[str, Any]], object]:
    """A list field's ids, the first few, comma separated (``-NONE-`` for none)."""
    get = _at(path)

    def read(rec: Mapping[str, Any]) -> object:
        """The column's value for one record."""
        raw = get(rec)
        if not isinstance(raw, (list, tuple)) or not raw:
            return "-NONE-"
        ids: list[str] = []
        for entry in raw:
            if isinstance(entry, dict):
                val = next(
                    (
                        v
                        for k, v in entry.items()
                        if isinstance(v, str) and k in ("id", "item", "creature", "name")
                    ),
                    None,
                )
            elif isinstance(entry, (list, tuple)):
                val = next((v for v in entry if isinstance(v, str)), None)
            else:
                val = entry if isinstance(entry, str) else None
            if val:
                ids.append(val)
        more = f" +{len(ids) - limit}" if len(ids) > limit else ""
        return ", ".join(ids[:limit]) + more if ids else "-NONE-"

    return read


def _effect(i: int) -> Callable[[Mapping[str, Any]], object]:
    """An ingredient's i-th effect (none shows empty)."""

    def read(rec: Mapping[str, Any]) -> object:
        """The column's value for one record."""
        effects = _at("data.effects")(rec)
        if not isinstance(effects, (list, tuple)) or i >= len(effects):
            return ""
        val = effects[i]
        if val in (None, -1, "None", "-1") or (isinstance(val, int) and val < 0):
            return ""
        return _words(str(val))

    return read


def _first_effect(field: str) -> Callable[[Mapping[str, Any]], object]:
    """A spell's (or enchantment's) first effect's ``field``."""

    def read(rec: Mapping[str, Any]) -> object:
        """The column's value for one record."""
        effects = rec.get("effects")
        if isinstance(effects, list) and effects and isinstance(effects[0], dict):
            return _words(str(effects[0].get(field) or ""))
        return ""

    return read


def _words(text: str) -> str:
    """``ShortBladeOneHand`` -> ``Short Blade One Hand``; ``ON_TOUCH`` -> ``On Touch``."""
    if "_" in text or text.isupper():
        return " ".join(w.capitalize() for w in text.split("_") if w)
    out = ""
    for i, ch in enumerate(text):
        if i and ch.isupper() and not text[i - 1].isupper():
            out += " "
        out += ch
    return out


def _enum(path: str) -> Callable[[Mapping[str, Any]], object]:
    """A reader for an enum field, in words (``LongBladeOneHand`` -> ``Long Blade One Hand``)."""
    get = _at(path)
    return lambda rec: _words(str(get(rec) or ""))


_NAME: Final[Column] = ("Name", _at("name"), "text", "The name the game shows")
_SCRIPT: Final[Column] = ("Script", _at("script"), "text", "The script it runs")
_WEIGHT: Final[Column] = ("Weight", _at("data.weight"), "num", "")
_VALUE: Final[Column] = ("Value", _at("data.value"), "num", "")
_ICON: Final[Column] = ("Inventory", _at("icon"), "text", "Its inventory icon")
_PERSISTS: Final[Column] = (
    "Persists",
    _flag("flags", "PERSISTENT"),
    "bool",
    "References persist (always loaded)",
)
_BLOCKED: Final[Column] = (
    "Blocked",
    _flag("flags", "BLOCKED"),
    "bool",
    "Blocked: the Construction Set will not change it",
)
_ENCH: Final[list[Column]] = [
    ("Enchanting", _at("enchanting"), "text", "Its enchantment"),
    ("Ench. pts", _at("data.enchantment"), "num", "Enchantment capacity"),
]
_USE_ITEM: Final[list[Column]] = [
    _NAME,
    _SCRIPT,
    _WEIGHT,
    _VALUE,
    ("Uses", _at("data.uses"), "num", ""),
    ("Quality", _at("data.quality"), "num", ""),
    _ICON,
]

#: Each tag's own columns, between the engine's id/count and its model and flags.
COLUMNS: Final[dict[str, list[Column]]] = {
    "ACTI": [_NAME, _SCRIPT],
    "ALCH": [
        _NAME,
        _SCRIPT,
        _WEIGHT,
        _VALUE,
        ("Autocalc", _flag("data.flags", "AUTO_CALCULATE"), "bool", ""),
        _ICON,
    ],
    "APPA": [
        ("Type", _enum("data.apparatus_type"), "text", ""),
        _NAME,
        _SCRIPT,
        _WEIGHT,
        _VALUE,
        ("Quality", _at("data.quality"), "num", ""),
        _ICON,
    ],
    "ARMO": [
        ("Type", _enum("data.armor_type"), "text", ""),
        _NAME,
        _SCRIPT,
        _WEIGHT,
        _VALUE,
        ("Health", _at("data.health"), "num", ""),
        *_ENCH,
        ("Rating", _at("data.armor_rating"), "num", "Armor rating"),
        _ICON,
    ],
    "BODY": [
        ("Type", _enum("data.bodypart_type"), "text", ""),
        ("Race", _at("race"), "text", ""),
        ("Part", _enum("data.part"), "text", ""),
        ("Female", _flag("data.flags", "FEMALE"), "bool", ""),
        ("Playable", lambda r: not _flag("data.flags", "NOT_PLAYABLE")(r), "bool", ""),
    ],
    "BOOK": [
        _NAME,
        _SCRIPT,
        _WEIGHT,
        _VALUE,
        ("Scroll", lambda r: str(_at("data.book_type")(r) or "").lower() == "scroll", "bool", ""),
        (
            "Teaches",
            lambda r: (
                ""
                if _at("data.skill")(r) in (None, -1, "None")
                else _words(str(_at("data.skill")(r)))
            ),
            "text",
            "The skill it raises when read",
        ),
        *_ENCH,
        _ICON,
    ],
    "CLOT": [
        ("Type", _enum("data.clothing_type"), "text", ""),
        _NAME,
        _SCRIPT,
        _WEIGHT,
        _VALUE,
        *_ENCH,
        _ICON,
    ],
    "CONT": [
        _NAME,
        _SCRIPT,
        ("Weight", _at("encumbrance"), "num", "How much it holds"),
        ("Organic", _flag("container_flags", "ORGANIC"), "bool", ""),
        ("Respawns", _flag("container_flags", "RESPAWNS"), "bool", ""),
        ("Item list", _names("inventory"), "text", ""),
    ],
    "DOOR": [
        _NAME,
        _SCRIPT,
        ("Open sound", _at("open_sound"), "text", ""),
        ("Close sound", _at("close_sound"), "text", ""),
    ],
    "INGR": [
        _NAME,
        _SCRIPT,
        _WEIGHT,
        _VALUE,
        *[(f"Effect {i + 1}", _effect(i), "text", "") for i in range(4)],
        _ICON,
    ],
    "LIGH": [
        _NAME,
        _SCRIPT,
        ("Sound", _at("sound"), "text", ""),
        _WEIGHT,
        _VALUE,
        ("Time", _at("data.time"), "num", "How long it burns (-1: forever)"),
        ("Radius", _at("data.radius"), "num", ""),
        _ICON,
    ],
    "LOCK": _USE_ITEM,
    "PROB": _USE_ITEM,
    "REPA": _USE_ITEM,
    "MISC": [
        _NAME,
        _SCRIPT,
        _WEIGHT,
        _VALUE,
        ("Key", _flag("data.flags", "KEY"), "bool", ""),
        _ICON,
    ],
    "STAT": [],
    "WEAP": [
        ("Type", _enum("data.weapon_type"), "text", ""),
        _NAME,
        _SCRIPT,
        _WEIGHT,
        ("Health", _at("data.health"), "num", ""),
        _VALUE,
        *_ENCH,
        ("Speed", _at("data.speed"), "num", ""),
        ("Reach", _at("data.reach"), "num", ""),
        (
            "Chop",
            lambda r: f"{_at('data.chop_min')(r)}-{_at('data.chop_max')(r)}",
            "text",
            "Chop damage, min-max",
        ),
        (
            "Slash",
            lambda r: f"{_at('data.slash_min')(r)}-{_at('data.slash_max')(r)}",
            "text",
            "Slash damage, min-max",
        ),
        (
            "Thrust",
            lambda r: f"{_at('data.thrust_min')(r)}-{_at('data.thrust_max')(r)}",
            "text",
            "Thrust damage, min-max",
        ),
        _ICON,
        (
            "Ignores resist",
            _flag("data.flags", "IGNORES_NORMAL_WEAPON_RESISTANCE"),
            "bool",
            "Ignores normal weapon resistance",
        ),
        ("Silver", _flag("data.flags", "SILVER"), "bool", ""),
    ],
    "NPC_": [
        _NAME,
        _SCRIPT,
        ("Level", _at("data.level"), "num", ""),
        ("Race", _at("race"), "text", ""),
        ("Female", _flag("npc_flags", "FEMALE"), "bool", ""),
        ("Class", _at("class"), "text", ""),
        ("Faction", _at("faction"), "text", ""),
        ("Rank", _at("data.rank"), "num", ""),
        ("Autocalc", _flag("npc_flags", "AUTO_CALCULATE"), "bool", ""),
        ("Essential", _flag("npc_flags", "ESSENTIAL"), "bool", ""),
        ("Respawns", _flag("npc_flags", "RESPAWN"), "bool", ""),
        ("Fight", _at("ai_data.fight"), "num", ""),
    ],
    "CREA": [
        _NAME,
        ("Type", _enum("data.creature_type"), "text", ""),
        _SCRIPT,
        ("Sound", _at("sound"), "text", "Its sound generator"),
        ("Level", _at("data.level"), "num", ""),
        ("Essential", _flag("creature_flags", "ESSENTIAL"), "bool", ""),
        ("Respawns", _flag("creature_flags", "RESPAWN"), "bool", ""),
        (
            "Movement",
            lambda r: ", ".join(
                w
                for f, w in (("WALKS", "Walk"), ("SWIMS", "Swim"), ("FLIES", "Fly"))
                if _flag("creature_flags", f)(r)
            ),
            "text",
            "",
        ),
        ("Weapon & shield", _flag("creature_flags", "WEAPON_AND_SHIELD"), "bool", ""),
        ("Biped", _flag("creature_flags", "BIPED"), "bool", ""),
        ("Soul", _at("data.soul"), "num", ""),
    ],
    "LEVC": [
        (
            "All levels",
            _flag("leveled_creature_flags", "CALCULATE_FROM_ALL_LEVELS"),
            "bool",
            "Calculate from all levels <= the player's",
        ),
        ("Chance none", _at("chance_none"), "num", ""),
        ("Creature list", _names("creatures"), "text", ""),
    ],
    "LEVI": [
        (
            "All levels",
            _flag("leveled_item_flags", "CALCULATE_FROM_ALL_LEVELS"),
            "bool",
            "Calculate from all levels <= the player's",
        ),
        (
            "Each item",
            _flag("leveled_item_flags", "CALCULATE_FOR_EACH_ITEM"),
            "bool",
            "Calculate for each item",
        ),
        ("Chance none", _at("chance_none"), "num", ""),
        ("Item list", _names("items"), "text", ""),
    ],
    "SPEL": [
        _NAME,
        ("Type", _enum("data.spell_type"), "text", ""),
        ("Cost", _at("data.cost"), "num", ""),
        ("Range", _first_effect("range"), "text", "Its first effect's range"),
        ("Autocalc", _flag("data.flags", "AUTO_CALCULATE"), "bool", ""),
        ("PC start", _flag("data.flags", "PC_START_SPELL"), "bool", ""),
    ],
    "ENCH": [
        ("Charge", _at("data.max_charge"), "num", ""),
        ("Type", _enum("data.enchant_type"), "text", ""),
        ("Cost", _at("data.cost"), "num", ""),
        ("Autocalc", lambda r: bool(_at("data.flags")(r)), "bool", ""),
    ],
}

#: The record type each tag reads as (tes3conv's ``type``).
_TYPES: Final[dict[str, str]] = {
    "ACTI": "Activator",
    "ALCH": "Alchemy",
    "APPA": "Apparatus",
    "ARMO": "Armor",
    "BODY": "Bodypart",
    "BOOK": "Book",
    "CLOT": "Clothing",
    "CONT": "Container",
    "DOOR": "Door",
    "INGR": "Ingredient",
    "LIGH": "Light",
    "LOCK": "Lockpick",
    "PROB": "Probe",
    "REPA": "RepairItem",
    "MISC": "MiscItem",
    "STAT": "Static",
    "WEAP": "Weapon",
    "NPC_": "Npc",
    "CREA": "Creature",
    "LEVC": "LeveledCreature",
    "LEVI": "LeveledItem",
    "SPEL": "Spell",
    "ENCH": "Enchanting",
}


def _show(value: object, kind: str) -> object:
    """A value as the table shows it: yes/no, numbers kept, floats to two places."""
    if kind == "bool":
        return "yes" if value else "no"
    if isinstance(value, float):
        return round(value, 2)
    if value is None:
        return ""
    return value


def _armor_class(settings: Mapping[str, float] | None) -> Column:
    """The Class column: Light, Medium or Heavy, by the load order's game settings."""
    from wraithguard.patch.armor import weight_class

    def read(rec: Mapping[str, Any]) -> object:
        """The column's value for one record."""
        data = rec.get("data") or {}
        return weight_class(
            str(data.get("armor_type") or ""), float(data.get("weight") or 0), settings
        )

    return (
        "Class",
        read,
        "text",
        "Weight class: the slot's base weight (iHelmWeight, iCuirassWeight...) times "
        "fLightMaxMod or fMedMaxMod, from the load order's game settings",
    )


def table(
    tag: str,
    versions: Sequence[Mapping[str, Mapping[str, Any]]],
    settings: Mapping[str, float] | None = None,
) -> dict[str, Any]:
    """The type's own columns for every record, read off the winning versions.

    Args:
        tag: The four-letter tag.
        versions: Per plugin in load order, its records of the tag by lower-case id
            (the last plugin with a record wins).
        settings: The game settings armor's weight class is worked out from
            (:func:`.armor.settings_from`); Morrowind's own without them.

    Returns:
        ``{columns: [{label, kind, title, path}], rows: {id lower: [value, ...]}}``,
        the columns persists and blocked last; empty for a type with no table here.
        ``path`` is the field a column shows as it is (so a cell can be edited in place),
        None for one worked out (a flag, a list, an enum in words).
    """
    tag = tag.upper().ljust(4, "_")
    cols = COLUMNS.get(tag)
    if cols is None:
        return {"columns": [], "rows": {}}
    if tag == "ARMO":
        cols = [cols[0], _armor_class(settings), *cols[1:]]
    cols = [*cols, _PERSISTS, _BLOCKED]
    want = _TYPES.get(tag)
    winners: dict[str, Mapping[str, Any]] = {
        key: rec
        for records in versions
        for key, rec in records.items()
        if want is None or rec.get("type") == want
    }
    rows = {
        key: [_show(_safe(get, rec), kind) for _label, get, kind, _title in cols]
        for key, rec in winners.items()
    }
    return {
        "columns": [
            {"label": label, "kind": kind, "title": title, "path": getattr(get, "path", None)}
            for label, get, kind, title in cols
        ],
        "rows": rows,
    }


def _safe(get: Callable[[Mapping[str, Any]], object], rec: Mapping[str, Any]) -> object:
    """A column's value, or empty when the record does not have its shape."""
    try:
        return get(rec)
    except (TypeError, ValueError, AttributeError):
        return ""


__all__ = ["COLUMNS", "table"]
