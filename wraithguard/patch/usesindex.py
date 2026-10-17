r"""The Use Report as an index: every string a record holds, by value, per plugin.

:func:`.uses.uses_in` finds where an id is used by walking every record of every plugin for
a string equal to it (and a word of a script's text). That is a scan of the whole load
order for each id. Here the same walk is done once per plugin for *every* string, and the
result - ``(value, using record, paths, count)`` - is kept in the record cache, so a use
report is one indexed query. What the index holds is exactly what ``uses_in`` would find
for each value, so the answers are the same.

Each record also gets a row for its own id with a count of 0 (the *definition* rows):
which plugins define a record is how the report tells a live use from one a later plugin
took out, and that cannot be had from the uses alone.

An id the index cannot answer for (it has characters a script word cannot, or it is too
long to have been indexed) is left to the scan: see :func:`indexable`.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import json
import re
from collections import Counter
from typing import TYPE_CHECKING, Any, Final

from wraithguard.patch.records import record_key

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator, Sequence

#: Strings longer than this are not ids (a book's text, a description): not indexed.
MAX_VALUE: Final = 64

#: A script's words, as the report's word rule sees them (``\w`` runs).
_WORD: Final = re.compile(r"\w+")

#: One row: ``(value, record type, record key, paths as JSON, count)``.
Row = tuple[str, str, str, str, int]


def indexable(rid: str) -> bool:
    r"""Whether the index answers for an id: it is a run of word characters, and short.

    The scan finds a script word with a boundary rule (``(?<!\w)id(?!\w)``) that for an
    id with a space or a dash is not the same as a word of the text; those go to the scan.

    Args:
        rid: The id.

    Returns:
        True when the index and the scan agree.
    """
    want = rid.strip()
    return bool(want) and len(want) <= MAX_VALUE and _WORD.fullmatch(want) is not None


def _strings(value: object, path: str) -> Iterator[tuple[str, str]]:
    """Every string in a record with its dotted path."""
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, dict):
        for k, v in value.items():
            yield from _strings(v, f"{path}.{k}" if path else str(k))
    elif isinstance(value, list):
        for i, v in enumerate(value):
            yield from _strings(v, f"{path}.{i}")


def index_rows(
    records: Sequence[dict[str, Any]],
    breathe: Callable[[], None] | None = None,
    every: int = 200,
) -> list[Row]:
    """The rows of one plugin's records.

    Args:
        records: The plugin's decoded records.
        breathe: Called every ``every`` records: a background indexer gives the other
            threads the interpreter there (it may raise to stop).
        every: How many records between calls.

    Returns:
        One row per ``(string value, using record)``: where in the record the value is
        (``references`` collapsed to one path, as the report does) and how many times; one
        per ``(word, script)`` with the path ``text``; and one definition row per record
        (its own id, count 0).
    """
    rows: list[Row] = []
    for n, rec in enumerate(records):
        if breathe is not None and n % every == 0:
            breathe()
        kind = rec.get("type")
        if not isinstance(kind, str) or kind == "Header":
            continue
        key = record_key(rec)
        per: dict[str, tuple[list[str], int]] = {}
        for path, text in _strings(rec, ""):
            low = text.lower()
            if not low or len(low) > MAX_VALUE or path == "id":
                continue
            paths, count = per.get(low, ([], 0))
            if path.startswith("references."):
                if "references" not in paths:
                    paths.append("references")
            else:
                paths.append(path)
            per[low] = (paths, count + 1)
        rows.extend((low, kind, key, json.dumps(p), n) for low, (p, n) in per.items())
        body = rec.get("text") if kind == "Script" else None
        if isinstance(body, str):
            rows.extend(
                (word, kind, key, '["text"]', n)
                for word, n in Counter(w.lower() for w in _WORD.findall(body)).items()
                if len(word) <= MAX_VALUE
            )
        rows.append((key.lower(), kind, key, '["id"]', 0))
    return rows


__all__ = ["MAX_VALUE", "Row", "index_rows", "indexable"]
