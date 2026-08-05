"""Line up the entries of a repeated field by what they *are*, not where they sit.

**The problem with comparing by position.** A cell's references, a leveled
list's entries, an NPC's inventory -- these are lists, and the flat diff
compares them whole: one row, "differs". Expand it and the obvious next move is
to compare entry 1 with entry 1, entry 2 with entry 2. That is wrong in the
most common case there is. A mod that *inserts* one item near the top shifts
everything after it by one, and an ordinal comparison then reports every
remaining entry as changed. The one real edit is buried in a hundred false
ones, which is indistinguishable from the tool not working.

**What identity means here.** Each kind of list has something that says which
entry is which, and it is not the index:

* a cell reference is identified by ``(mast_index, refr_index)`` -- the object
  instance, which is stable across plugins by design;
* a leveled list entry by its item *and* its level, because the same item
  legitimately appears at several levels;
* an inventory entry by the item id, because the count is the thing that gets
  edited;
* a faction reaction by the faction it is toward.

Match on those and an inserted entry shows up as an insertion, an edited one as
an edit, and everything else stays quiet.

**Row order.** Entries are placed by the same rule the dialogue resolver uses:
each plugin's list is walked in order, and an entry not seen before is inserted
after the one that preceded it in *that* plugin. So a mod's new entry appears
where the mod put it, rather than being appended to the bottom where nobody
would look for it.

The idea is from ``decoder/content_alignment.cpp`` in yampt (MIT,
Rafał Wierzchoś, 2016-2026), which does it over raw subrecords with per-record
anchor/key rules. Working from tes3conv's decoded JSON means the entries are
already structured, so what is left is deciding what identifies each kind --
which is the part that has to be right.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Final

from wraithguard.patch.status import (
    ABSENT,
    ConflictAll,
    ConflictThis,
    conflict_all,
    conflict_this,
)

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

#: Fields whose entries are ``[id, level]`` pairs. The level is part of the
#: identity, not a value: one item at two levels is two entries, and treating
#: them as one would merge a mod's level-1 sword with its level-20 sword.
LEVELLED: Final[frozenset[str]] = frozenset({"items", "creatures"})

#: Fields whose entries are ``[count, id]`` pairs. Here the count *is* the
#: value being edited, so only the id identifies the entry.
COUNTED: Final[frozenset[str]] = frozenset({"inventory"})

#: Per-field identity, for entries that are objects. The value is the sequence
#: of keys that together say which entry this is; everything else is the
#: content being compared.
BY_KEYS: Final[dict[str, tuple[str, ...]]] = {
    "references": ("mast_index", "refr_index"),
    "reactions": ("faction",),
    "travel_destinations": ("cell",),
}


@dataclass(frozen=True, slots=True)
class Row:
    """One entry of a repeated field, across every plugin.

    Attributes:
        label: How the entry is named in the panel.
        values: Its value in each plugin, in load order.
            :data:`~wraithguard.patch.status.ABSENT` where that plugin's list
            does not contain it -- which is the whole point: added and removed
            entries are visible as such.
        overall: The entry across every plugin.
        per_plugin: One status per plugin, in load order.
    """

    label: str
    values: tuple[Any, ...]
    overall: ConflictAll
    per_plugin: tuple[ConflictThis, ...]

    @property
    def present(self) -> tuple[bool, ...]:
        """Which plugins have this entry at all.

        Returns:
            One flag per plugin, in load order.
        """
        return tuple(value is not ABSENT for value in self.values)


# Any: an entry is whatever tes3conv decoded -- a pair, an object, a string.
def identity(field: str, entry: Any) -> str:  # noqa: ANN401
    """What says which entry this is, within its field.

    Args:
        field: The field's name, which decides the rule.
        entry: One entry of the list.

    Returns:
        A stable key. Falls back to the entry's whole content, which makes
        equal entries align and unequal ones not -- the safe direction, since
        it can only fail to spot that two entries are the same thing.
    """
    if field in LEVELLED and isinstance(entry, (list, tuple)) and len(entry) == 2:
        return f"{entry[0]} @ {entry[1]}"
    if field in COUNTED and isinstance(entry, (list, tuple)) and len(entry) == 2:
        return str(entry[1])
    if isinstance(entry, dict):
        wanted = BY_KEYS.get(field)
        if wanted:
            return "|".join(str(entry.get(name, "")) for name in wanted)
        found = entry.get("id")
        if isinstance(found, str) and found:
            return found
    if isinstance(entry, str):
        return entry
    return _stable(entry)


# Any: as identity -- the entry's shape is the field's business, not ours.
def label_for(field: str, entry: Any) -> str:  # noqa: ANN401
    """How an entry should read in the panel.

    The identity is often not the useful name: a reference is identified by its
    object index, which nobody recognises, but it carries the object's id.

    Args:
        field: The field's name.
        entry: One entry of the list.

    Returns:
        A short human label.
    """
    if isinstance(entry, dict):
        found = entry.get("id")
        if isinstance(found, str) and found:
            return found
    if field in COUNTED and isinstance(entry, (list, tuple)) and len(entry) == 2:
        return str(entry[1])
    return identity(field, entry)


# Any: deliberately total, so an unknown entry shape degrades rather than raises.
def _stable(value: Any) -> str:  # noqa: ANN401
    """A comparable rendering of any entry.

    Args:
        value: Anything tes3conv produced.

    Returns:
        JSON with sorted keys, or ``repr`` for what JSON cannot take.
    """
    try:
        return json.dumps(value, sort_keys=True)
    except (TypeError, ValueError):
        return repr(value)


def _merged_order(base: Sequence[str], incoming: Sequence[str]) -> list[str]:
    """Weave one plugin's entry order into the order built so far.

    Entries the two lists share are the anchors; anything new is placed where
    that plugin put it, relative to the nearest anchor before it. Everything
    already placed keeps its order.

    **Why this is a merge and not repeated insertion.** The first version
    inserted each new key into a shared list and then rebuilt a key-to-index
    map so the next insertion knew where to go -- an O(n) rebuild per entry, so
    O(n^2) overall. That is invisible on an inventory of nine items and ruinous
    on a cell's references, where one exterior cell can carry thousands. This
    walks each list once.

    Args:
        base: The order established by the plugins read so far.
        incoming: This plugin's entry keys, in its own order, deduplicated.

    Returns:
        The merged order.
    """
    if not base:
        return list(incoming)
    known = set(base)
    out: list[str] = []
    placed: set[str] = set()
    at = 0
    for key in incoming:
        if key in known:
            # Catch the result up to this shared anchor, then let it stand.
            while at < len(base):
                nxt = base[at]
                at += 1
                if nxt not in placed:
                    out.append(nxt)
                    placed.add(nxt)
                if nxt == key:
                    break
        elif key not in placed:
            out.append(key)
            placed.add(key)
    for key in base[at:]:
        if key not in placed:
            out.append(key)
            placed.add(key)
    return out


def align(
    field: str,
    per: Mapping[str, Any],
    plugins: Sequence[str],
) -> list[Row]:
    """Line one repeated field up across every plugin that defines it.

    Args:
        field: The field's name, which decides what identifies an entry.
        per: Plugin name to that plugin's value for the field. A plugin whose
            record has no such field, or whose value is not a list, contributes
            nothing rather than an empty list -- "did not say" and "said it is
            empty" are different claims.
        plugins: The plugins, **in load order**.

    Returns:
        One :class:`Row` per distinct entry, in merged list order.
    """
    order: list[str] = []
    seen: list[dict[str, Any]] = []
    labels: dict[str, str] = {}

    for plugin in plugins:
        entries = per.get(plugin)
        if not isinstance(entries, list):
            seen.append({})
            continue
        mine: dict[str, Any] = {}
        theirs: list[str] = []
        for entry in entries:
            key = identity(field, entry)
            if key not in mine:
                theirs.append(key)
            mine[key] = entry
            labels.setdefault(key, label_for(field, entry))
        order = _merged_order(order, theirs)
        seen.append(mine)

    rows: list[Row] = []
    for key in order:
        values = [mine.get(key, ABSENT) for mine in seen]
        comparable = [value if value is ABSENT else _stable(value) for value in values]
        rows.append(
            Row(
                label=labels.get(key, key),
                values=tuple(values),
                overall=conflict_all(comparable, skip_absent=False),
                per_plugin=tuple(conflict_this(comparable, skip_absent=False)),
            )
        )
    return rows


# Any: one plugin's value for a field, which may be anything at all.
def alignable(field: str, value: Any) -> bool:  # noqa: ANN401
    """Whether a field holds *entries*, as opposed to merely being a list.

    Not every list is a repeated field. A landscape's ``grid`` is ``[-27, 6]``
    and a reference's ``translation`` is three floats: fixed-arity tuples that
    happen to use list syntax. Lining those up entry by entry is meaningless,
    and offering them invites the question "what is the point of this" -- which
    is exactly what the first version earned.

    So a field qualifies only when it is one we know is repeated, or when its
    items are objects, which fixed tuples never are.

    Args:
        field: The field's name.
        value: One plugin's value for it.

    Returns:
        ``True`` when the field holds a variable number of entries.
    """
    if not isinstance(value, list) or not value:
        return False
    if field in LEVELLED or field in COUNTED or field in BY_KEYS:
        return True
    # Objects and names are entries. A list of bare numbers is a coordinate, a
    # rotation, or a colour -- a fixed tuple written with list syntax, where
    # position means something and identity does not.
    return all(isinstance(entry, (dict, str)) for entry in value)


def alignable_fields(keys: Sequence[str], per: Mapping[str, Mapping[str, Any]]) -> list[str]:
    """Which of a record's fields have entries to line up.

    Args:
        keys: The record's fields.
        per: Plugin name to that plugin's field values.

    Returns:
        The field names, in the order given.
    """
    return [key for key in keys if any(alignable(key, values.get(key)) for values in per.values())]
