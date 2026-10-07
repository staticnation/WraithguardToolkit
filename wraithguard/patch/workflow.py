"""The Editor's workflow helpers: answers worked out over the load order and the pool.

- :func:`leveled_roll` - what a leveled list gives at a chosen level, as chances.
- :func:`who_can_say` - the NPCs a dialogue response's speaker conditions let say it.
- :func:`filter_choices` - what a dialogue condition can be (its types, functions,
  comparisons), for the condition editor.
- :func:`script_words` - what Script Edit completes: functions, keywords, globals.
- :func:`script_refs` - every script naming a word (a variable, an id).
- :func:`search_all` / :func:`replace_all` - text found in every record type at once,
  and replaced (OpenMW-CS's global search).

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import copy
import re
from typing import TYPE_CHECKING, Any

from wraithguard.patch.merge import FieldValue, set_at

if TYPE_CHECKING:
    from collections.abc import Iterable

    from wraithguard.patch.editor import EditorSession, Found

#: How deep a leveled list's lists of lists are followed.
MAX_DEPTH = 8


# ---- leveled lists ---------------------------------------------------------------------


def _flags(value: object) -> set[str]:
    """A flags field as the reader gives it (``A | B``, or a list) -> its names."""
    if isinstance(value, (list, tuple)):
        return {str(v).strip().upper() for v in value}
    return {p.strip().upper() for p in str(value or "").split("|") if p.strip()}


def roll_entries(record_type: str, record: dict[str, Any], level: int) -> dict[str, float]:
    """One leveled list rolled once at a level: each entry's chance (none: ``""``).

    As the game rolls one: ``chance_none`` percent gives nothing; otherwise the entries
    at or below the level are the candidates - all of them with "calculate from all
    levels <= PC's level", else only those at the highest of their levels - and one of
    them is taken, each as likely as the others (an id listed twice, twice as likely).

    Args:
        record_type: ``LeveledItem`` or ``LeveledCreature``.
        record: The list.
        level: The player's level.

    Returns:
        ``{id: chance}`` (0 - 1), with ``""`` the chance of nothing.
    """
    field = "items" if record_type == "LeveledItem" else "creatures"
    flags = _flags(
        record.get("leveled_item_flags" if field == "items" else "leveled_creature_flags")
    )
    entries = [
        (str(e[0]), int(e[1]))
        for e in record.get(field) or []
        if isinstance(e, (list, tuple)) and len(e) >= 2 and str(e[0]).strip()
    ]
    none = min(max(int(record.get("chance_none") or 0), 0), 100) / 100
    eligible = [(i, lv) for i, lv in entries if lv <= level]
    if not eligible:
        return {"": 1.0}
    if "CALCULATE_FROM_ALL_LEVELS" not in flags:
        top = max(lv for _, lv in eligible)
        eligible = [(i, lv) for i, lv in eligible if lv == top]
    out: dict[str, float] = {"": none} if none else {}
    each = (1 - none) / len(eligible)
    for i, _lv in eligible:
        out[i] = out.get(i, 0.0) + each
    return out


def leveled_roll(session: EditorSession, found: Found, level: int) -> dict[str, Any]:
    """What a leveled list gives at a level, lists inside it rolled too.

    Args:
        session: The session (the lists as the patch leaves them).
        found: The list.
        level: The player's level.

    Returns:
        ``{"level", "outcomes": [{"id", "tag", "chance"}] (most likely first; ``id``
        ``""`` for nothing), "nested": [list ids rolled inside]}``.
    """
    from wraithguard.patch.verify import as_written

    tag = found.tag
    nested: list[str] = []

    def roll(
        rec: dict[str, Any], rtype: str, weight: float, depth: int, out: dict[str, float]
    ) -> None:
        """Add one list's outcomes, weighted, following lists inside it."""
        for rid, chance in roll_entries(rtype, rec, level).items():
            if not rid:
                out[""] = out.get("", 0.0) + weight * chance
                continue
            inner = session.find(tag, rid) if depth < MAX_DEPTH else None
            if inner is not None and inner.record_type == rtype:
                nested.append(inner.key)
                roll(as_written(session, inner), rtype, weight * chance, depth + 1, out)
            else:
                out[rid] = out.get(rid, 0.0) + weight * chance

    totals: dict[str, float] = {}
    roll(as_written(session, found), found.record_type, 1.0, 0, totals)
    ranked = sorted(
        ((k, v) for k, v in totals.items() if v > 0), key=lambda kv: (-kv[1], kv[0].lower())
    )
    outcomes = [{"id": k, "chance": round(v, 6)} for k, v in ranked]
    return {"level": level, "outcomes": outcomes, "nested": sorted(set(nested), key=str.lower)}


# ---- dialogue ----------------------------------------------------------------------------


def filter_choices() -> dict[str, list[str]]:
    """What a dialogue condition can be: its types, functions and comparisons by name."""
    from wraithguard.esp.enums import FilterComparison, FilterFunction, FilterType

    def names(enum: Iterable[Any]) -> list[str]:
        """An enum's members as the records name them (``None_`` is ``None``)."""
        return [m.name.rstrip("_") for m in enum]

    return {
        "types": names(FilterType),
        "functions": names(FilterFunction),
        "comparisons": names(FilterComparison),
    }


def who_can_say(session: EditorSession, info: dict[str, Any], limit: int = 500) -> dict[str, Any]:
    """The NPCs a response's speaker conditions let say it.

    Checked: the speaker's id, race, class, faction (and rank at least the one asked),
    sex, and the conditions naming ids (not this id, faction, class, race). The rest - the
    cell, functions, variables, the journal - depend on the game being played, and are
    listed as not checked.

    Args:
        session: The session.
        info: The response (as the patch leaves it).
        limit: The most NPCs listed.

    Returns:
        ``{"npcs": [{"id", "name"}], "count", "capped", "unchecked": [what was not]}``.
    """
    want = {
        k: str(info.get(k) or "").strip().lower()
        for k in ("speaker_id", "speaker_race", "speaker_class", "speaker_faction")
    }
    raw_data = info.get("data")
    data: dict[str, Any] = raw_data if isinstance(raw_data, dict) else {}
    raw_rank = data.get("speaker_rank", -1)
    rank = int(raw_rank) if isinstance(raw_rank, (int, float)) else -1
    sex = str(data.get("speaker_sex") or "Any")
    nots: dict[str, set[str]] = {
        "NotId": set(),
        "NotFaction": set(),
        "NotClass": set(),
        "NotRace": set(),
    }
    unchecked: list[str] = []
    if str(info.get("speaker_cell") or "").strip():
        unchecked.append(f"the cell ({info.get('speaker_cell')})")
    for f in info.get("filters") or []:
        kind = str((f or {}).get("filter_type") or "")
        if kind in nots:
            nots[kind].add(str(f.get("id") or "").lower())
        elif kind and kind != "None":
            label = f.get("id") or f.get("function") or ""
            unchecked.append(f"{kind} {label}".strip())
    winners: dict[str, dict[str, Any]] = {}
    for plugin in session.order:
        winners.update(
            {k: r for k, r in session._records(plugin, "NPC_").items() if r.get("type") == "Npc"}
        )
    npcs: list[dict[str, str]] = []
    count = 0
    for key, npc in sorted(winners.items()):
        if want["speaker_id"] and key != want["speaker_id"]:
            continue
        race, cls, fac = (str(npc.get(k) or "").lower() for k in ("race", "class", "faction"))
        if want["speaker_race"] and race != want["speaker_race"]:
            continue
        if want["speaker_class"] and cls != want["speaker_class"]:
            continue
        if want["speaker_faction"]:
            npc_rank = int((npc.get("data") or {}).get("rank", 0) or 0)
            if fac != want["speaker_faction"] or (rank >= 0 and npc_rank < rank):
                continue
        female = "FEMALE" in _flags(npc.get("npc_flags"))
        if (sex == "Male" and female) or (sex == "Female" and not female):
            continue
        if key in nots["NotId"] or fac in nots["NotFaction"]:
            continue
        if cls in nots["NotClass"] or race in nots["NotRace"]:
            continue
        count += 1
        if len(npcs) < limit:
            npcs.append({"id": str(npc.get("id") or key), "name": str(npc.get("name") or "")})
    return {"npcs": npcs, "count": count, "capped": count > limit, "unchecked": unchecked}


# ---- scripts -----------------------------------------------------------------------------


def script_words(session: EditorSession) -> dict[str, list[str]]:
    """What Script Edit completes: the game's functions, the keywords, the globals.

    Args:
        session: The session (for the load order's globals).

    Returns:
        ``{"functions", "keywords", "globals"}``.
    """
    from wraithguard.mwscript.check import KEYWORDS
    from wraithguard.mwscript.compiler_data import CUSTOM_FUNCTIONS, FUNCTIONS

    functions = sorted({f[0] for f in FUNCTIONS} | {f[0] for f in CUSTOM_FUNCTIONS}, key=str.lower)
    return {
        "functions": functions,
        "keywords": sorted(KEYWORDS, key=str.lower),
        "globals": session.script_globals(),
    }


def script_refs(session: EditorSession, word: str, limit: int = 1000) -> dict[str, Any]:
    """Every script naming a word - a variable, an id - as the patch leaves them.

    A word is matched whole (any case), in the code, not in a comment; ``"quoted"`` ids
    are matched too.

    Args:
        session: The session.
        word: The word.
        limit: The most lines answered.

    Returns:
        ``{"word", "hits": [{"script", "line", "text"}], "scripts": n, "capped"}``.
    """
    w = word.strip()
    if not w:
        return {"word": w, "hits": [], "scripts": 0, "capped": False}
    pat = re.compile(r"(?<![\w])" + re.escape(w) + r"(?![\w])", re.IGNORECASE)
    hits: list[dict[str, Any]] = []
    scripts: set[str] = set()
    total = 0
    for key, text in sorted(_script_texts(session).items()):
        for n, line in enumerate(text.splitlines(), 1):
            code = line.split(";", 1)[0]
            if pat.search(code):
                total += 1
                scripts.add(key)
                if len(hits) < limit:
                    hits.append({"script": key, "line": n, "text": line.strip()})
    return {"word": w, "hits": hits, "scripts": len(scripts), "capped": total > limit}


def _script_texts(session: EditorSession) -> dict[str, str]:
    """Every script's text, as the patch leaves it (its id as the record spells it)."""
    out: dict[str, str] = {}
    for plugin in session.order:
        for rec in session._records(plugin, "SCPT").values():
            out[str(rec.get("id") or "")] = str(rec.get("text") or "")
    for (rtype, key), choices in session.queue.fields.items():
        if rtype == "Script":
            for c in choices:
                if isinstance(c, FieldValue) and c.path == "text":
                    out[key] = str(c.value)
    for made in session.queue.new_records:
        if made.record_type == "Script":
            out[made.key] = str(made.record.get("text") or "")
    return out


# ---- global search -----------------------------------------------------------------------


def _strings(value: object, path: str = "") -> Iterable[tuple[str, str]]:
    """Every text field of a record (lists' entries included): ``(path, text)``."""
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, dict):
        for k, v in value.items():
            if k not in ("type", "references", "bytecode"):
                yield from _strings(v, f"{path}.{k}" if path else str(k))
    elif isinstance(value, list):
        for i, v in enumerate(value):
            yield from _strings(v, f"{path}.{i}" if path else str(i))


def search_all(
    session: EditorSession, text: str, whole: bool = False, limit: int = 2000
) -> dict[str, Any]:
    """Text found in every record type at once (OpenMW-CS's global search).

    Args:
        session: The session (records as the patch leaves them).
        text: What to find (any case).
        whole: Only whole words.
        limit: The most hits answered.

    Returns:
        ``{"hits": [{"tag", "id", "path", "value"}], "count", "capped"}``.
    """
    from wraithguard.patch.editor import TAG_TO_TYPE

    t = text.strip()
    if not t:
        return {"hits": [], "count": 0, "capped": False}
    pat = re.compile(
        (r"(?<![\w])" + re.escape(t) + r"(?![\w])") if whole else re.escape(t), re.IGNORECASE
    )
    hits: list[dict[str, str]] = []
    count = 0
    for tag, rtype in sorted(TAG_TO_TYPE.items()):
        if tag in ("TES3", "LAND", "PGRD"):
            continue
        for key, rec in sorted(_winners(session, tag, rtype).items()):
            for path, value in _strings(rec):
                if pat.search(value):
                    count += 1
                    if len(hits) < limit:
                        hits.append(
                            {
                                "tag": tag,
                                "id": str(rec.get("id") or rec.get("name") or key),
                                "path": path,
                                "value": value,
                            }
                        )
    return {"hits": hits, "count": count, "capped": count > limit}


def _winners(session: EditorSession, tag: str, rtype: str) -> dict[str, dict[str, Any]]:
    """A type's records as the patch leaves them: winning versions, typed values applied."""
    out: dict[str, dict[str, Any]] = {}
    for plugin in session.order:
        out.update(
            {k: r for k, r in session._records(plugin, tag).items() if r.get("type") == rtype}
        )
    for (t, key), choices in session.queue.fields.items():
        if t == rtype and key.lower() in out:
            rec = copy.deepcopy(out[key.lower()])
            for c in choices:
                if isinstance(c, FieldValue):
                    set_at(rec, c.path, c.value)
            out[key.lower()] = rec
    for made in session.queue.new_records:
        if made.record_type == rtype:
            out[made.key.lower()] = dict(made.record)
    return out


def replace_all(
    session: EditorSession, hits: list[dict[str, str]], text: str, by: str, whole: bool = False
) -> dict[str, Any]:
    """The text replaced in the fields found, as one change (one Ctrl+Z).

    Only fields that are text of their own are changed (a list's entry changes the list
    it is in); a record's id is never changed (that is Search & Replace's, which repoints
    what names it).

    Args:
        session: The session.
        hits: The fields to change (:func:`search_all`'s, the ones chosen).
        text: What to replace.
        by: What it becomes.
        whole: Only whole words.

    Returns:
        ``{"changed": n, "failed": [{"tag", "id", "path", "error"}]}``.
    """
    pat = re.compile(
        (r"(?<![\w])" + re.escape(text) + r"(?![\w])") if whole else re.escape(text),
        re.IGNORECASE,
    )
    changed = 0
    failed: list[dict[str, str]] = []
    with session.batch():
        for h in hits:
            error = _replace_one(session, h, pat, by)
            if error is None:
                changed += 1
            else:
                failed.append(
                    {**{k: str(h.get(k, "")) for k in ("tag", "id", "path")}, "error": error}
                )
    return {"changed": changed, "failed": failed}


def _replace_one(
    session: EditorSession, hit: dict[str, str], pat: re.Pattern[str], by: str
) -> str | None:
    """One field's text replaced (via :meth:`set_field`); the refusal as text, or None."""
    from wraithguard.patch.editor import EditorError
    from wraithguard.patch.merge import value_at
    from wraithguard.patch.verify import as_written

    path = str(hit.get("path") or "")
    if path in ("id", "name") and hit.get("tag") == "CELL":
        return "a cell's name is its id here"
    if path == "id":
        return "an id is changed with Search & Replace (it repoints what names it)"
    found = session.find(str(hit.get("tag") or ""), str(hit.get("id") or ""))
    if found is None:
        return "no plugin defines it"
    # A list's entry: the list is the field.
    top = path
    parts = path.split(".")
    for i, part in enumerate(parts):
        if part.isdigit():
            top = ".".join(parts[:i])
            break
    rec = as_written(session, found)
    value, there = value_at(rec, top)
    if not there:
        return "the field is not there"
    new_value = _replaced(value, pat, by)
    if new_value == value:
        return "the text is no longer there"
    try:
        session.set_field(found, top, new_value)
    except EditorError as exc:
        return str(exc)
    return None


def _replaced(value: object, pat: re.Pattern[str], by: str) -> object:
    """A copy of a value with the text replaced in every string in it."""
    if isinstance(value, str):
        return pat.sub(lambda _m: by, value)
    if isinstance(value, list):
        return [_replaced(v, pat, by) for v in value]
    if isinstance(value, dict):
        return {k: _replaced(v, pat, by) for k, v in value.items()}
    return value
