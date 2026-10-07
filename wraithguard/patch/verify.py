"""The Editor's verifier: checks over what the patch carries, as the patch leaves it.

OpenMW-CS's verifier is the reference for *what* is worth checking; the checks here are
our own, over Wraithguard's records (the tes3 crate's shapes). Each record the patch
carries is taken as the patch would write it - the winning version, or the one taken
whole, with every queued field applied - and checked:

- **Missing IDs:** a field that names another record (a script, a race, an item in an
  inventory, a spell, a body part, a sound, a leveled list's entries, an AI package's
  target or cell, a travel destination, a dialogue filter...) naming one no plugin of
  the load order defines and the patch does not make; and references the patch places
  (or gives another object) naming one.
- **Leveled lists:** an entry with no id, or a list with no entries.
- **Scripts:** a script the patch changes that does not compile.
- **Dialogue:** a response that has lost its place in its topic; a journal's stages
  (one quest name, one entry per index, text for each).
- **Path grids:** points on top of each other, points no link reaches, links to points
  it does not have, and an exterior's points under the land.
- **Per type** (:func:`record_checks`), as OpenMW-CS's verifier has them: names and
  descriptions, attributes and skills listed twice, a race's sizes, a region's weather,
  effect lists, enchantment cost against charge and an item's capacity, a game
  setting's type, and the files body parts, sounds, birthsigns and magic effects name.
- **Required IDs:** the globals every game needs.

Each finding is ``{"level", "check", "tag", "id", "path", "message"}`` - ``level``
``error`` (the game will trip on it) or ``warning``.
"""

from __future__ import annotations

import copy
from typing import TYPE_CHECKING, Any

from wraithguard.patch.merge import FieldValue, set_at, value_at

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable, Iterator

    from wraithguard.patch.editor import EditorSession, Found

#: Records an inventory or a leveled item list may name.
ITEM_TAGS = (
    "ALCH",
    "APPA",
    "ARMO",
    "BOOK",
    "CLOT",
    "INGR",
    "LIGH",
    "LOCK",
    "MISC",
    "PROB",
    "REPA",
    "WEAP",
    "LEVI",
)
#: Records a leveled creature list may name.
ACTOR_TAGS = ("CREA", "NPC_", "LEVC")
#: Records that can be placed in a cell (what a reference, or an AI package, may name).
OBJECT_TAGS = (*ITEM_TAGS, "ACTI", "CONT", "DOOR", "STAT", "BODY", "CREA", "NPC_", "LEVC")

#: A dialogue filter's type -> what its ``id`` names (the types naming variables, a
#: function or a cell are not ids of records).
FILTER_TAGS: dict[str, tuple[str, ...]] = {
    "Global": ("GLOB",),
    "Journal": ("DIAL",),
    "Item": ITEM_TAGS,
    "Dead": ("NPC_", "CREA"),
    "NotId": ("NPC_", "CREA"),
    "NotFaction": ("FACT",),
    "NotClass": ("CLAS",),
    "NotRace": ("RACE",),
}

#: Globals every game needs (OpenMW-CS's mandatory ids).
REQUIRED_GLOBALS = ("Day", "DaysPassed", "GameHour", "Month", "PCRace")

#: Per record type: ``(where, tags)`` - ``where`` a dotted path, where ``*`` is every
#: entry of a list and a number one place in an entry; ``tags`` what it may name.
REFERENCES: dict[str, tuple[tuple[str, tuple[str, ...]], ...]] = {
    "Npc": (
        ("ai_packages.*.target", (*ACTOR_TAGS, *OBJECT_TAGS)),
        ("race", ("RACE",)),
        ("class", ("CLAS",)),
        ("faction", ("FACT",)),
        ("head", ("BODY",)),
        ("hair", ("BODY",)),
        ("spells.*", ("SPEL",)),
        ("inventory.*.1", ITEM_TAGS),
    ),
    "Creature": (
        ("ai_packages.*.target", (*ACTOR_TAGS, *OBJECT_TAGS)),
        ("spells.*", ("SPEL",)),
        ("inventory.*.1", ITEM_TAGS),
    ),
    "Container": (("inventory.*.1", ITEM_TAGS),),
    "LeveledItem": (("items.*.0", ITEM_TAGS),),
    "LeveledCreature": (("creatures.*.0", ACTOR_TAGS),),
    "Door": (("open_sound", ("SOUN",)), ("close_sound", ("SOUN",))),
    "Light": (("sound", ("SOUN",)),),
    "Race": (("spells.*", ("SPEL",)),),
    "Birthsign": (("spells.*", ("SPEL",)),),
    "Region": (("sleep_creature", ("LEVC",)), ("sounds.*.0", ("SOUN",))),
    "Faction": (("reactions.*.faction", ("FACT",)),),
    "MagicEffect": (
        ("cast_sound", ("SOUN",)),
        ("bolt_sound", ("SOUN",)),
        ("hit_sound", ("SOUN",)),
        ("area_sound", ("SOUN",)),
        ("cast_visual", ("STAT",)),
        ("bolt_visual", ("STAT",)),
        ("hit_visual", ("STAT",)),
        ("area_visual", ("STAT",)),
    ),
    "Bodypart": (("race", ("RACE",)),),
    "Armor": (
        ("biped_objects.*.male_bodypart", ("BODY",)),
        ("biped_objects.*.female_bodypart", ("BODY",)),
    ),
    "Clothing": (
        ("biped_objects.*.male_bodypart", ("BODY",)),
        ("biped_objects.*.female_bodypart", ("BODY",)),
    ),
    "SoundGen": (("sound", ("SOUN",)), ("creature", ("CREA",))),
    "DialogueInfo": (
        ("speaker_id", ("NPC_", "CREA")),
        ("speaker_race", ("RACE",)),
        ("speaker_class", ("CLAS",)),
        ("speaker_faction", ("FACT",)),
        ("player_faction", ("FACT",)),
    ),
    "Cell": (("region", ("REGN",)),),
}
#: Fields any record type may have that name another record.
COMMON: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("script", ("SCPT",)),
    ("enchanting", ("ENCH",)),
)

Finding = dict[str, str]


def _at(value: object, parts: list[str], path: str) -> Iterator[tuple[str, object]]:
    """Every ``(path, value)`` a reference pattern reaches in a value."""
    if not parts:
        yield path, value
        return
    head, rest = parts[0], parts[1:]
    if head == "*":
        if isinstance(value, list):
            for i, item in enumerate(value):
                yield from _at(item, rest, f"{path}.{i}" if path else str(i))
    elif head.isdigit():
        if isinstance(value, list) and int(head) < len(value):
            yield from _at(value[int(head)], rest, f"{path}.{head}")
    elif isinstance(value, dict) and head in value:
        yield from _at(value[head], rest, f"{path}.{head}" if path else head)


def references(
    record_type: str, record: dict[str, Any]
) -> Iterator[tuple[str, str, tuple[str, ...]]]:
    """The ids a record names: ``(path, id, tags it may be)``, empty ones left out.

    Args:
        record_type: Its type.
        record: The record.
    """
    for where, tags in (*COMMON, *REFERENCES.get(record_type, ())):
        for path, value in _at(record, where.split("."), ""):
            if isinstance(value, str) and value.strip():
                yield path, value.strip(), tags
    # A dialogue filter's id, by what its type names.
    for i, f in enumerate(record.get("filters") or [] if record_type == "DialogueInfo" else []):
        named = FILTER_TAGS.get(str((f or {}).get("filter_type")))
        name = str((f or {}).get("id") or "").strip()
        if named and name:
            yield f"filters.{i}.id", name, named
    # The interior cells travel and AI packages go to (an exterior's is "").
    for where in ("travel_destinations.*.cell", "ai_packages.*.cell"):
        for path, value in _at(record, where.split("."), ""):
            if isinstance(value, str) and value.strip():
                yield path, value.strip(), ("CELL",)


def missing_ids(
    tag: str,
    key: str,
    record_type: str,
    record: dict[str, Any],
    exists: Callable[[tuple[str, ...], str], bool],
) -> list[Finding]:
    """A record's fields that name records nothing defines.

    Args:
        tag: Its tag.
        key: Its id.
        record_type: Its type.
        record: It, as the patch leaves it.
        exists: ``(tags, id) -> bool``: whether a record of one of the tags has the id.

    Returns:
        The findings.
    """
    return [
        {
            "level": "error",
            "check": "Missing ID",
            "tag": tag,
            "id": key,
            "path": path,
            "message": f"{path} names {name}, which no plugin defines ({'/'.join(tags)})",
        }
        for path, name, tags in references(record_type, record)
        if not exists(tags, name)
    ]


def leveled(tag: str, key: str, record_type: str, record: dict[str, Any]) -> list[Finding]:
    """A leveled list's empty entries, or no entries at all.

    Args:
        tag: Its tag.
        key: Its id.
        record_type: Its type.
        record: It, as the patch leaves it.

    Returns:
        The findings.
    """
    field = {"LeveledItem": "items", "LeveledCreature": "creatures"}.get(record_type)
    if field is None:
        return []
    entries = record.get(field) or []
    out: list[Finding] = []
    if not entries:
        out.append(_finding("warning", "Leveled list", tag, key, field, "the list is empty"))
    for i, entry in enumerate(entries):
        name = entry[0] if isinstance(entry, list) and entry else None
        if not isinstance(name, str) or not name.strip():
            out.append(
                _finding("error", "Leveled list", tag, key, f"{field}.{i}", "an entry with no id")
            )
    return out


def path_grid(tag: str, key: str, points: list[Any], edges: list[Any]) -> list[Finding]:
    """A path grid's points on top of each other, points no link reaches, bad links.

    Args:
        tag: ``PGRD``.
        key: The grid's cell.
        points: ``[[x, y, z]]``.
        edges: ``[[a, b]]``.

    Returns:
        The findings.
    """
    out: list[Finding] = []
    seen: dict[tuple[float, ...], int] = {}
    for i, p in enumerate(points):
        at = tuple(float(c) for c in p[:3])
        if at in seen:
            out.append(
                _finding(
                    "warning",
                    "Path grid",
                    tag,
                    key,
                    f"points.{i}",
                    f"point {i} is on point {seen[at]} ({at[0]:g}, {at[1]:g}, {at[2]:g})",
                )
            )
        else:
            seen[at] = i
    linked: set[int] = set()
    for a, b in edges:
        if not (0 <= a < len(points) and 0 <= b < len(points)):
            out.append(
                _finding(
                    "error", "Path grid", tag, key, "edges", f"a link {a}-{b} to a missing point"
                )
            )
            continue
        linked.update((a, b))
    if len(points) > 1:
        out += [
            _finding("warning", "Path grid", tag, key, f"points.{i}", f"point {i} is unlinked")
            for i in range(len(points))
            if i not in linked
        ]
    return out


def dialogue_chain(topic: dict[str, Any], touched: set[str]) -> list[Finding]:
    """Responses the patch touches that have lost their place in their topic.

    Args:
        topic: :meth:`.EditorSession.topic`'s answer (``orphan``: a response whose
            predecessor is not in the topic; the engine reads it last).
        touched: The responses (INFO ids, lower-case) the patch carries.

    Returns:
        The findings.
    """
    return [
        _finding(
            "error",
            "Dialogue",
            "INFO",
            str(r.get("id", "")),
            "prev_id",
            f"its place in {topic.get('id')} names a response the topic does not have:"
            " the game reads it last",
        )
        for r in topic.get("responses") or []
        if str(r.get("id", "")).lower() in touched and r.get("orphan")
    ]


def journal(topic: dict[str, Any]) -> list[Finding]:
    """A journal topic's stages: one quest name, one entry per index, every one with text.

    Args:
        topic: :meth:`.EditorSession.topic`'s answer, for a journal.

    Returns:
        The findings.
    """
    tid = str(topic.get("id", ""))
    entries = topic.get("responses") or []
    out: list[Finding] = []
    names = [r for r in entries if r.get("quest") == "Name"]
    if len(names) > 1:
        out.append(_finding("error", "Journal", "DIAL", tid, "quest", "more than one quest name"))
    stages = [r for r in entries if r.get("quest") != "Name"]
    if names and not stages:
        out.append(_finding("warning", "Journal", "DIAL", tid, "", "a quest name with no stages"))
    seen: dict[int, str] = {}
    for r in stages:
        index = int(r.get("disposition") or 0)
        rid = str(r.get("id", ""))
        if index in seen:
            out.append(
                _finding(
                    "error",
                    "Journal",
                    "DIAL",
                    tid,
                    rid,
                    f"index {index} twice ({seen[index]}, {rid})",
                )
            )
        seen.setdefault(index, rid)
        if not str(r.get("text") or "").strip():
            out.append(
                _finding("warning", "Journal", "DIAL", tid, rid, f"stage {index} has no text")
            )
    return out


def _flag(record: dict[str, Any], path: str, name: str) -> bool:
    """Whether a flags field (as the reader gives one: names joined) has a flag."""
    value, _ = value_at(record, path)
    return name.upper() in str(value or "").upper().replace(" ", "").split("|")


def _asset(prefix: str, path: str) -> list[str]:
    """The VFS paths an asset field may resolve to (a texture: its ``.dds`` too)."""
    rel = path.strip().replace("\\", "/").lower()
    full = rel if rel.startswith(prefix) else prefix + rel
    out = [full]
    if prefix in ("textures/", "icons/") and not full.endswith(".dds"):
        out.append(full.rsplit(".", 1)[0] + ".dds")
    return out


def record_checks(
    tag: str,
    key: str,
    record_type: str,
    record: dict[str, Any],
    file_exists: Callable[[str], bool] | None = None,
) -> list[Finding]:
    """OpenMW-CS's per-type checks, on one record as the patch leaves it.

    Names and descriptions missing; a class's or faction's attribute or skill listed
    twice; a race's height and weight; a region's weather chances; spells, enchantments
    and potions with no effects; an enchantment costing more than its charge; a game
    setting whose value is not of the type its name says; a body part, sound, birthsign
    or magic effect whose file is missing (when ``file_exists`` can tell).

    Args:
        tag: Its tag.
        key: Its id.
        record_type: Its type.
        record: The record.
        file_exists: ``(vfs path) -> bool``, or None not to look for files.

    Returns:
        The findings.
    """
    out: list[Finding] = []
    check = record_type

    def add(level: str, path: str, message: str) -> None:
        """One finding on this record."""
        out.append(_finding(level, check, tag, key, path, message))

    def text(path: str) -> str:
        """A text field, trimmed."""
        value, _ = value_at(record, path)
        return str(value or "").strip()

    def asset(path: str, prefix: str, needed: bool) -> None:
        """A file field: missing, or naming a file the data folders do not have."""
        name = text(path)
        if not name:
            if needed:
                add("error", path, "no file is given")
            return
        if file_exists is not None and not any(file_exists(p) for p in _asset(prefix, name)):
            add("error", path, f"{name} is in no data folder or archive")

    raw_data = record.get("data")
    data: dict[str, Any] = raw_data if isinstance(raw_data, dict) else {}
    if record_type in ("Class", "Faction", "Race", "Region", "Birthsign", "Spell") and not text(
        "name"
    ):
        add("warning", "name", "the name is missing")
    if record_type == "Class":
        if data.get("attribute1") == data.get("attribute2"):
            add("error", "data.attribute2", "the same attribute is listed twice")
        skills = [
            data[f"{k}{i}"] for k in ("major", "minor") for i in range(1, 6) if f"{k}{i}" in data
        ]
        for twice in sorted({str(x) for x in skills if skills.count(x) > 1}):
            add("error", "data", f"{twice} is listed twice among the skills")
        if _flag(record, "data.flags", "PLAYABLE") and not text("description"):
            add("warning", "description", "a playable class has no description")
    elif record_type == "Faction":
        attrs = list(data.get("favored_attributes") or [])
        if len(attrs) == 2 and attrs[0] == attrs[1]:
            add("error", "data.favored_attributes", "the same attribute is listed twice")
        skills = list(data.get("favored_skills") or [])
        for twice in sorted({str(x) for x in skills if skills.count(x) > 1}):
            add("error", "data.favored_skills", f"{twice} is listed twice")
    elif record_type == "Race":
        if not text("description"):
            add("warning", "description", "the description is missing")
        for sex, i in (("male", 0), ("female", 1)):
            height = list(data.get("height") or [1, 1])
            weight = list(data.get("weight") or [1, 1])
            if i < len(height) and float(height[i]) <= 0:
                add("error", "data.height", f"the {sex} height is not above 0")
            if i < len(weight) and float(weight[i]) < 0:
                add("error", "data.weight", f"the {sex} weight is below 0")
    elif record_type == "Region":
        chances = record.get("weather_chances") or {}
        total = sum(int(v) for v in chances.values()) if isinstance(chances, dict) else 100
        if total != 100:
            add("error", "weather_chances", f"the weather chances add up to {total}, not 100")
    elif record_type == "Birthsign":
        if not text("description"):
            add("warning", "description", "the description is missing")
        asset("texture", "textures/", True)
    elif record_type == "Bodypart":
        asset("mesh", "meshes/", True)
        if text("data.bodypart_type") in ("", "Skin") and not text("race"):
            add("warning", "race", "a skin part with no race")
    elif record_type == "Sound":
        asset("sound_path", "sound/", True)
    elif record_type == "SoundGen":
        if not text("sound"):
            add("error", "sound", "no sound is given")
    elif record_type == "MagicEffect":
        if not text("description"):
            add("warning", "description", "the description is missing")
        asset("icon", "icons/", True)
        asset("texture", "textures/", False)
    elif record_type == "Skill":
        if not text("description"):
            add("warning", "description", "the description is missing")
    elif record_type == "GameSetting":
        _gmst_type(key, record, add)
    if record_type in ("Spell", "Enchanting", "Alchemy") and not record.get("effects"):
        add("error", "effects", "no magic effects")
    if record_type == "Enchanting":
        cost, charge = int(data.get("cost") or 0), int(data.get("max_charge") or 0)
        if str(data.get("enchant_type")) != "ConstantEffect" and cost > charge:
            add("warning", "data.cost", f"each use costs {cost}, more than its charge of {charge}")
    return out


def _gmst_type(key: str, record: dict[str, Any], add: Callable[[str, str, str], None]) -> None:
    """A game setting's value against its name: ``f`` a float, ``i`` an integer, ``s`` text."""
    want = {"f": "Float", "i": "Integer", "s": "String"}.get(key[:1].lower())
    value = record.get("value")
    have = str(value.get("type")) if isinstance(value, dict) else ""
    if want and have and have != want:
        add("error", "value", f"{key} is a {have} setting; its name says {want}")


def enchant_capacity(
    tag: str, key: str, record: dict[str, Any], cost: int | None, mult: float | None
) -> list[Finding]:
    """An item whose enchantment costs more than the item can carry.

    The capacity is as the game's enchanting counts it: the item's enchantment points
    times the ``fEnchantmentMult`` setting.

    Args:
        tag: The item's tag.
        key: Its id.
        record: It.
        cost: Its enchantment's cost, or None when it has none (or it is not found).
        mult: ``fEnchantmentMult`` from the load order, or None.

    Returns:
        The findings.
    """
    points = (record.get("data") or {}).get("enchantment")
    if cost is None or mult is None or not isinstance(points, (int, float)):
        return []
    capacity = int(points * mult)
    if cost > capacity:
        return [
            _finding(
                "warning",
                "Enchantment",
                tag,
                key,
                "enchanting",
                f"its enchantment costs {cost}; the item carries {capacity}"
                f" ({points:g} points x fEnchantmentMult {mult:g})",
            )
        ]
    return []


def under_ground(
    tag: str, key: str, points: list[Any], heights: list[list[float]], margin: float = 16.0
) -> list[Finding]:
    """An exterior path grid's points below the land.

    Args:
        tag: ``PGRD``.
        key: The grid's cell.
        points: ``[[x, y, z]]``, relative to the cell's south-west corner.
        heights: The cell's 65 x 65 land heights, row 0 the south edge.
        margin: How far below counts (the land between vertices is two triangles; the
            heights are read bilinearly).

    Returns:
        The findings.
    """
    step = 8192 / 64
    out: list[Finding] = []
    for i, p in enumerate(points):
        x, y, z = (float(c) for c in p[:3])
        cx = min(max(x / step, 0.0), 64.0)
        cy = min(max(y / step, 0.0), 64.0)
        c0, r0 = min(int(cx), 63), min(int(cy), 63)
        fx, fy = cx - c0, cy - r0
        h = (
            heights[r0][c0] * (1 - fx) * (1 - fy)
            + heights[r0][c0 + 1] * fx * (1 - fy)
            + heights[r0 + 1][c0] * (1 - fx) * fy
            + heights[r0 + 1][c0 + 1] * fx * fy
        )
        if z < h - margin:
            out.append(
                _finding(
                    "warning",
                    "Path grid",
                    tag,
                    key,
                    f"points.{i}",
                    f"point {i} is {h - z:.0f} units under the ground",
                )
            )
    return out


def required_ids(exists: Callable[[tuple[str, ...], str], bool]) -> list[Finding]:
    """The globals every game needs, missing from the load order and the patch."""
    return [
        _finding(
            "error",
            "Required ID",
            "GLOB",
            name,
            "",
            f"the global {name} every game needs is missing",
        )
        for name in REQUIRED_GLOBALS
        if not exists(("GLOB",), name)
    ]


def _finding(level: str, check: str, tag: str, key: str, path: str, message: str) -> Finding:
    """One finding."""
    return {"level": level, "check": check, "tag": tag, "id": key, "path": path, "message": message}


def as_written(session: EditorSession, found: Found) -> dict[str, Any]:
    """A record as the patch would write it: its base, every queued field applied.

    Args:
        session: The session.
        found: The record.

    Returns:
        A copy.
    """
    versions = dict(found.versions)
    whole = next(
        (
            s.plugin
            for s in session.queue.selections
            if (s.record_type, s.key) == (found.record_type, found.key)
        ),
        None,
    )
    record = copy.deepcopy(versions.get(whole, found.record) if whole else found.record)
    for path, choice in session._choices(found).items():
        if isinstance(choice, FieldValue):
            value: object = choice.value
        else:
            value, there = value_at(versions.get(choice.plugin, {}), path)
            if not there:
                continue
        set_at(record, path, copy.deepcopy(value))
    return record


def verify(
    session: EditorSession, file_exists: Callable[[str], bool] | None = None
) -> dict[str, Any]:
    """Every check over what the patch carries.

    Args:
        session: The session (read on the caller's thread, as :meth:`find` is).
        file_exists: ``(vfs path) -> bool`` for the files records name, or None not to
            look (the Editor gives the load order's data folders and archives).

    Returns:
        ``{"findings": [...], "checked": n}`` - ``checked`` how many records.
    """
    known: dict[str, set[str]] = {}

    def ids_of(tag: str) -> set[str]:
        """Every id of a tag: the plugins' and the patch's own."""
        if tag not in known:
            have: set[str] = set()
            for plugin in session.order:
                have |= set(session._records(plugin, tag))
            have |= {m.key.lower() for m in session.queue.new_records if _tag(m.record_type) == tag}
            known[tag] = have
        return known[tag]

    def exists(tags: tuple[str, ...], name: str) -> bool:
        """Whether one of the tags has the id."""
        return any(name.lower() in ids_of(t) for t in tags)

    findings: list[Finding] = required_ids(exists)
    checked = 0
    infos: list[str] = []
    journals: set[str] = set()
    for entry in _carried(session):
        found = session.find(entry["tag"], entry["id"])
        if found is None:
            continue
        checked += 1
        record = as_written(session, found)
        tag, key, rtype = found.tag, found.key, found.record_type
        findings += missing_ids(tag, key, rtype, record, exists)
        findings += leveled(tag, key, rtype, record)
        findings += record_checks(tag, key, rtype, record, file_exists)
        if str(record.get("enchanting") or "").strip():
            findings += enchant_capacity(
                tag, key, record, _enchant_cost(session, record), _gmst(session, "fEnchantmentMult")
            )
        if rtype == "Script" and _changes_text(entry):
            findings += _script(session, found, record)
        if rtype == "DialogueInfo":
            infos.append(key.lower())
        if rtype == "Dialogue" and str(record.get("dialogue_type")) == "Journal":
            journals.add(key)
        if rtype == "PathGrid":
            cell = _grid_cell(found, record)
            view = session.pathgrid_view(cell)
            findings += path_grid("PGRD", key, view["points"], view["edges"])
            heights = None if cell.startswith("int:") else _land(session, view.get("grid"))
            if heights is not None:
                findings += under_ground("PGRD", key, view["points"], heights)
    findings += _placed(session, exists)
    topics: dict[str, set[str]] = {}
    if infos:
        owner = _info_topics(session)
        for info in infos:
            if info in owner:
                topics.setdefault(owner[info], set()).add(info)
    for topic in sorted(set(topics) | journals):
        findings += _chain(session, topic, topics.get(topic, set()))
    return {"findings": findings, "checked": checked}


def _placed(
    session: EditorSession, exists: Callable[[tuple[str, ...], str], bool]
) -> list[Finding]:
    """References the patch places, or gives another object, naming one nothing defines."""
    out: list[Finding] = []
    for new in session.queue.new_refs:
        name = str(new.fields.get("id") or "")
        if name and not exists(OBJECT_TAGS, name):
            out.append(
                _finding(
                    "error",
                    "Reference",
                    "CELL",
                    new.cell,
                    new.uid,
                    f"places {name}, which no plugin defines",
                )
            )
    for edit in session.queue.ref_edits:
        name = str(edit.changes.get("id") or "")
        if name and not exists(OBJECT_TAGS, name):
            out.append(
                _finding(
                    "error",
                    "Reference",
                    "CELL",
                    edit.cell,
                    f"{edit.origin}#{edit.refr_index}",
                    f"becomes {name}, which no plugin defines",
                )
            )
    return out


def _gmst(session: EditorSession, name: str) -> float | None:
    """A numeric game setting as the load order leaves it (the last plugin's), or None."""
    value: float | None = None
    for plugin in session.order:
        rec = session._records(plugin, "GMST").get(name.lower())
        raw = (rec or {}).get("value")
        if isinstance(raw, dict) and isinstance(raw.get("data"), (int, float)):
            value = float(raw["data"])
    return value


def _enchant_cost(session: EditorSession, record: dict[str, Any]) -> int | None:
    """The cost of the enchantment an item names, as the patch leaves it, or None."""
    found = session.find("ENCH", str(record.get("enchanting") or "").strip())
    if found is None:
        return None
    cost = (as_written(session, found).get("data") or {}).get("cost")
    return int(cost) if isinstance(cost, (int, float)) else None


def _land(session: EditorSession, grid: object) -> list[list[float]] | None:
    """An exterior cell's land heights, the last plugin's, or None (no land: flat)."""
    import struct

    from wraithguard.tes3fields.landscape import LandscapeDecodeError, decode_vertex_heights

    if not isinstance(grid, (list, tuple)) or len(grid) != 2:
        return None
    want = [int(grid[0]), int(grid[1])]
    found: dict[str, Any] | None = None
    for plugin in session.order:
        for rec in session._records(plugin, "LAND").values():
            if [int(v) for v in (rec.get("grid") or [None, None])] == want:
                found = rec
    if found is None:
        return None
    heights = found.get("vertex_heights") or {}
    data = heights.get("data")
    if data is None:
        return None
    if isinstance(data, list):  # rows of numbers, as some readers give them
        flat = [int(v) for row in data for v in (row if isinstance(row, list) else [row])]
        data = struct.pack(f"<{len(flat)}b", *flat)
    try:
        return decode_vertex_heights(data, float(heights.get("offset") or 0.0))
    except (LandscapeDecodeError, TypeError, ValueError):
        return None


def _chain(session: EditorSession, topic: str, touched: set[str]) -> list[Finding]:
    """:func:`dialogue_chain`, and :func:`journal` for a journal, over one topic.

    Nothing when the topic cannot be read.
    """
    try:
        read = session.topic(topic)
    except Exception:  # noqa: BLE001 - a topic not read is not a finding
        return []
    out = dialogue_chain(read, touched)
    if str(read.get("type")) == "Journal":
        out += journal(read)
    return out


def _info_topics(session: EditorSession) -> dict[str, str]:
    """Each response's topic, lower-case id -> topic id.

    The topic before it in its plugin, the last plugin's word winning; for a response
    the patch makes, its own.
    """
    from wraithguard.patch.records import DIALOGUE_TYPE, INFO_TYPE, record_key

    owner: dict[str, str] = {}
    for plugin in session.order:
        topic = ""
        for record in session._dialogue(plugin):
            if record.get("type") == DIALOGUE_TYPE:
                topic = record_key(record)
            elif record.get("type") == INFO_TYPE and topic:
                owner[record_key(record).lower()] = topic
    for made in session.queue.new_records:
        if made.record_type == INFO_TYPE and made.topic:
            owner[made.key.lower()] = made.topic
    return owner


def _tag(record_type: str) -> str:
    """A record type's tag."""
    from wraithguard.patch.editor import tag_of

    return tag_of(record_type)


def _carried(session: EditorSession) -> Iterable[dict[str, Any]]:
    """The records the patch carries (not references), from :meth:`pending`."""
    return [p for p in session.pending() if p.get("tag") and not p.get("ref") and not p.get("new")]


def _changes_text(entry: dict[str, Any]) -> bool:
    """Whether a pending script's changes reach its text (or it is new or whole)."""
    if entry.get("whole") or entry.get("made"):
        return True
    return any(str(c.get("path", "")) == "text" for c in entry.get("changes") or [])


def _script(session: EditorSession, found: Found, record: dict[str, Any]) -> list[Finding]:
    """A script's compile errors, as the Script Edit window reports them."""
    try:
        view = session.script_view(found, str(record.get("text") or ""))
    except Exception as exc:  # noqa: BLE001 - no compiler here: said, not raised
        return [_finding("warning", "Script", found.tag, found.key, "text", f"not checked: {exc}")]
    errors = [m for m in view["compile"]["messages"] if m["level"] == "error"]
    errors += [f for f in view["findings"] if f["level"] == "error"]
    return [
        _finding("error", "Script", found.tag, found.key, f"text:{m['line']}", str(m["message"]))
        for m in errors
    ]


def _grid_cell(found: Found, record: dict[str, Any]) -> str:
    """The cell a path grid record belongs to, as :meth:`pathgrid_view` takes it."""
    grid = (record.get("data") or {}).get("grid") or [0, 0]
    if list(grid) == [0, 0]:
        return f"int:{record.get('cell') or found.key}"
    return f"({grid[0]}, {grid[1]})"
