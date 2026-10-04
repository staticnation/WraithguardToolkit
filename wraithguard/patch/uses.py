"""Where a record is used: the Construction Set's Use Report, over a load order.

Every plugin is read whole and every record searched for the id: as a value anywhere in
it (a leveled list's entry, an inventory's item, a spell list, an enchantment, a sound,
a dialogue filter, a reference placed in a cell), and as a word in a script's text. A
record is only walked when its text could hold the id at all - ``str(record)`` is made
in C and is far cheaper than walking the record in Python - and the plugins are read on
threads, which run side by side on free-threaded Python.

Each use says which plugin's version of the using record it is in and whether that
version is the one the load order uses (the last plugin with the record), so a use a
later mod already took out is told apart from a live one. A cell is not last-wins - each
plugin's CELL record adds references to it - so a reference placed by any plugin counts.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from wraithguard.patch.records import record_key

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator, Sequence
    from pathlib import Path

#: Record types whose text is source code naming ids as words.
_SCRIPT_TYPES = frozenset({"Script"})


@dataclass
class Use:
    """One record that uses the id.

    Attributes:
        plugin: The plugin whose version of the record it is.
        record_type: The using record's type.
        key: Its key.
        paths: Where in it the id is (dotted paths; ``references`` collapsed).
        count: How many times (a cell's references: how many are placed).
        wins: Whether this version is the one the load order uses.
    """

    plugin: str
    record_type: str
    key: str
    paths: list[str] = field(default_factory=list)
    count: int = 0
    wins: bool = True


def _walk(value: object, want: str, path: str) -> Iterator[str]:
    """The dotted paths in a record at which a string equal to ``want`` (lower) sits."""
    if isinstance(value, str):
        if value.lower() == want:
            yield path
    elif isinstance(value, dict):
        for k, v in value.items():
            yield from _walk(v, want, f"{path}.{k}" if path else str(k))
    elif isinstance(value, list):
        for i, v in enumerate(value):
            yield from _walk(v, want, f"{path}.{i}")


def uses_in(
    plugin: str,
    records: Sequence[dict[str, Any]],
    record_type: str,
    rid: str,
) -> list[Use]:
    """The uses of one id in one plugin's records.

    Args:
        plugin: The plugin's name.
        records: Its decoded records.
        record_type: The used record's type (its own record is not a use of itself).
        rid: The id.

    Returns:
        One :class:`Use` per using record.
    """
    want = rid.strip().lower()
    word = re.compile(rf"(?<![\w]){re.escape(want)}(?![\w])", re.IGNORECASE)
    out: list[Use] = []
    for rec in records:
        kind = rec.get("type")
        if not isinstance(kind, str) or kind == "Header":
            continue
        if want not in str(rec).lower():
            continue
        key = record_key(rec)
        if kind == record_type and key.lower() == want:
            continue
        paths: list[str] = []
        count = 0
        for p in _walk(rec, want, ""):
            if p.startswith("references."):
                count += 1
                if "references" not in paths:
                    paths.append("references")
            elif p != "id":
                count += 1
                paths.append(p)
        if kind in _SCRIPT_TYPES:
            text = rec.get("text")
            if isinstance(text, str):
                hits = len(word.findall(text))
                if hits:
                    count += hits
                    paths.append("text")
        if count:
            out.append(Use(plugin, kind, key, paths, count))
    return out


def use_report(
    plugins: Sequence[tuple[str, Path]],
    read: Callable[[Path], list[dict[str, Any]]],
    record_type: str,
    rid: str,
) -> list[Use]:
    """Every use of an id over a load order, in load order.

    Args:
        plugins: The load order, ``(name, path)``.
        read: Reads all of one plugin's records.
        record_type: The used record's type.
        rid: The id.

    Returns:
        The uses, each marked with whether its version wins.
    """
    from wraithguard.parallel import read_all

    def one(item: tuple[str, Path]) -> tuple[list[Use], set[tuple[str, str]]]:
        """One plugin's uses, and every (type, key) it has (for who wins)."""
        name, path = item
        try:
            records = read(path)
        except (OSError, ValueError):
            return [], set()
        keys = {(str(r.get("type")), record_key(r).lower()) for r in records}
        return uses_in(name, records, record_type, rid), keys

    found = read_all(plugins, one)
    last: dict[tuple[str, str], str] = {}
    for (name, _path), (_uses, keys) in zip(plugins, found, strict=True):
        for k in keys:
            last[k] = name
    out: list[Use] = []
    for uses, _keys in found:
        for u in uses:
            u.wins = u.record_type == "Cell" or last.get((u.record_type, u.key.lower())) == u.plugin
            out.append(u)
    return out


__all__ = ["Use", "use_report", "uses_in"]
