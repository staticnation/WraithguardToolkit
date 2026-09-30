"""From the cell viewer to the conflict viewer: which row a record the viewer names is.

The cell viewer's object inspector (ORI) names records by their four-byte tag and id --
the base record of a clicked object, its cell, its owner, its script, a container's
contents. :func:`find_conflict_row` finds that record among the conflict viewer's rows,
which name the type the way tes3conv does (``Npc``, ``MiscItem``, ``Cell``).
"""

from __future__ import annotations

import json
import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence
    from pathlib import Path

_TAG = re.compile(r"[A-Z0-9_]{4}")


def record_type_name(tag: str) -> str:
    """The conflict viewer's type name for a four-byte tag (``"NPC_"`` -> ``"Npc"``).

    Args:
        tag: The record's tag, as the viewer sends it.

    Returns:
        The type name, or ``""`` for a tag the reader does not model.
    """
    import wraithguard.esp  # noqa: F401  (fills the registry)
    from wraithguard.esp.record import REGISTRY

    cls = REGISTRY.get(tag.encode("ascii", errors="ignore"))
    return cls.__name__ if cls is not None else ""


def record_tag(type_name: str) -> str:
    """The four-byte tag for a conflict viewer type name (``"Npc"`` -> ``"NPC_"``).

    Args:
        type_name: The type as a conflict row names it.

    Returns:
        The tag, or ``""`` for a type the reader does not model.
    """
    import wraithguard.esp  # noqa: F401  (fills the registry)
    from wraithguard.esp.record import REGISTRY

    for tag, cls in REGISTRY.items():
        if cls.__name__.lower() == type_name.lower():
            return tag.decode("ascii", errors="replace")
    return ""


def cell_preview_spec(type_name: str, rid: str) -> str:
    """What the cell viewer is asked to show for a conflict row: ``find:<tag>:<id>``.

    Args:
        type_name: The row's type.
        rid: The row's id.

    Returns:
        The spec, or ``""`` for a record nothing stands in a cell for (a spell, a topic).
    """
    tag = record_tag(type_name)
    placeable = {
        "ACTI",
        "ALCH",
        "APPA",
        "ARMO",
        "BOOK",
        "CELL",
        "CLOT",
        "CONT",
        "CREA",
        "DOOR",
        "INGR",
        "LEVC",
        "LEVI",
        "LIGH",
        "LOCK",
        "MISC",
        "NPC_",
        "PROB",
        "REPA",
        "STAT",
        "WEAP",
    }
    if tag not in placeable or not rid or any(c in rid for c in "\r\n\x00"):
        return ""
    return f"find:{tag}:{rid}"


def plugin_spec(plugin: str) -> str:
    """What the cell viewer is asked for to show a plugin: ``plugin:<name>``.

    Its cells on the map, and the plugin reviewed in the cells opened from there.

    Args:
        plugin: The plugin's file name.

    Returns:
        The spec, or ``""`` for a name that is empty or would break the line.
    """
    name = plugin.strip()
    if not name or any(c in name for c in "\r\n\x00"):
        return ""
    return f"plugin:{name}"


def parse_request(body: bytes) -> tuple[str, str]:
    """The ``{tag, id}`` the viewer posts, checked.

    Args:
        body: The POST body.

    Returns:
        ``(tag, id)``; the tag may be empty (the id alone is then matched).

    Raises:
        ValueError: For anything that is not such a request.
    """
    try:
        req = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError("not a record request") from exc
    if not isinstance(req, dict):
        raise ValueError("not a record request")
    tag = str(req.get("tag") or "")
    rid = str(req.get("id") or "").strip()
    if tag and not _TAG.fullmatch(tag):
        raise ValueError("bad tag")
    if not rid or len(rid) > 256 or any(c in rid for c in "\r\n\x00"):
        raise ValueError("bad id")
    return tag, rid


def find_conflict_row(rows: Sequence[Mapping[str, object]], tag: str, rid: str) -> int | None:
    """The index of the row for ``tag``/``rid``: its type and id, or else its id alone.

    Ids compare without case, as the game compares them. A tag names a type the
    viewer only guessed at for some fields (a key is usually a misc item), so a row
    with the id under another type is the fallback.

    Args:
        rows: The conflict viewer's rows (``type``, ``id``, ...).
        tag: The record's tag, or ``""``.
        rid: The record's id.

    Returns:
        The row's index, or None.
    """
    want_type = record_type_name(tag).lower() if tag else ""
    want_id = rid.lower()
    fallback: int | None = None
    for i, row in enumerate(rows):
        if str(row.get("id", "")).lower() != want_id:
            continue
        if not want_type or str(row.get("type", "")).lower() == want_type:
            return i
        if fallback is None:
            fallback = i
    return fallback


def with_extra_plugins(lines: list[str], plugins: list[Path]) -> list[str]:
    """``lines`` (an openmw.cfg) with ``plugins`` loaded last.

    Each one's folder becomes the last ``data=`` path, so its own file wins, and a
    ``content=`` line follows the rest (any line already naming it dropped, so it does not
    load twice).

    Args:
        lines: The cfg's lines.
        plugins: The plugin files.

    Returns:
        The new lines.
    """
    if not plugins:
        return list(lines)
    names = {p.name.lower() for p in plugins}
    out = [
        ln
        for ln in lines
        if not (
            ln.strip().lower().startswith("content=")
            and ln.split("=", 1)[1].strip().lower() in names
        )
    ]
    last_data = max(
        (i for i, ln in enumerate(out) if ln.strip().lower().startswith("data=")), default=-1
    )
    dirs: list[str] = []
    for p in plugins:
        d = str(p.resolve().parent)
        if d not in dirs:
            dirs.append(d)
    data_lines = [f'data="{d}"' for d in dirs]
    out = out[: last_data + 1] + data_lines + out[last_data + 1 :]
    out += [f"content={p.name}" for p in plugins]
    return out
