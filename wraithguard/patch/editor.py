"""The viewer's editor: records looked up, shown and changed, into the patch queue.

The cell viewer has an Editor mode (``viewer-shell/ui/src/50_wg_editor.js``) laid out
like the Construction Set: an Object Window, a Cell View, the render window, and a
dialog for each record. The viewer only draws it; the records, the edits and the
writing are Wraithguard's, and this is that half, holding no widgets so it is tested
without a display:

- :meth:`EditorSession.find` reads a record from every plugin that defines it (the
  viewer says which, from the load order it loaded), in load order;
- :meth:`EditorSession.view` is what the record dialog shows: each field (dotted paths,
  as the conflict viewer's diff panel names them), its value in the winning plugin,
  what the queue would write instead, and what the schema knows about it;
- :meth:`EditorSession.set_field` / :meth:`EditorSession.revert` change the queue - the
  same :class:`~wraithguard.patch.queue.PatchQueue` the conflict viewer fills, so the
  Patch window reviews and writes editor changes beside conflict choices;
- every change is written to a **journal** on disk, the whole queue, so neither a viewer
  that runs out of memory nor a Wraithguard that crashes loses the work:
  :meth:`EditorSession.restore` puts it back.

The queue is not thread safe, and the loopback server answers on its own thread: the
caller runs :meth:`view`, :meth:`set_field`, :meth:`revert`, :meth:`pending` and
:meth:`restore` on the thread that owns the queue (the Tk thread). :meth:`find`, which
only reads plugins, may run anywhere.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import json
import threading
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Final

from wraithguard.logging_setup import get_logger
from wraithguard.patch.enums import enum_options
from wraithguard.patch.fieldtypes import field_kind, flag_options, flags_name, int_bounds
from wraithguard.patch.merge import IDENTITY, FieldChoice, FieldValue, value_at
from wraithguard.patch.records import Selection, record_key
from wraithguard.patch.refedit import RefEdit
from wraithguard.tes3fields.naming import TYPE_TO_TAG

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence
    from pathlib import Path

    from wraithguard.patch.queue import PatchQueue

LOG = get_logger(__name__)

#: Record tag (four characters, as the files have it: ``NPC_``) -> tes3conv ``type``.
TAG_TO_TYPE: Final[dict[str, str]] = {tag.ljust(4, "_"): name for name, tag in TYPE_TO_TAG.items()}


def tag_of(record_type: str) -> str:
    """A tes3conv ``type``'s four-character tag (``Npc`` -> ``NPC_``), or ``""``."""
    tag = TYPE_TO_TAG.get(record_type, "")
    return tag.ljust(4, "_") if tag else ""


#: Fields the editor shows but does not change. ``references`` is one list per cell
#: (the diff panel keeps lists whole): writing it would re-assert every reference that
#: version of the cell lists, freezing them against later mods, to change one. A
#: changed reference is written alone instead (:mod:`.refedit`, merge_to_master's rule).
READ_ONLY: Final[frozenset[str]] = frozenset({"references"})

_JOURNAL_VERSION: Final = 1


class EditorError(ValueError):
    """A change the editor refuses, with the reason to show."""


@dataclass(frozen=True)
class Found:
    """One record, as every plugin that defines it has it.

    Attributes:
        tag: Its four-letter tag (``NPC_``).
        record_type: Its tes3conv ``type`` (``Npc``).
        key: Its key, as the record spells it (:func:`.records.record_key`).
        versions: ``(plugin, record)`` for each plugin defining it, in load order;
            the last wins.
    """

    tag: str
    record_type: str
    key: str
    versions: tuple[tuple[str, dict[str, Any]], ...]

    @property
    def winner(self) -> str:
        """The plugin whose version the game uses."""
        return self.versions[-1][0]

    @property
    def record(self) -> dict[str, Any]:
        """The winning version."""
        return self.versions[-1][1]


def _flatten(record: Mapping[str, Any], parent: str = "") -> dict[str, Any]:
    """A record's fields as dotted paths, lists kept whole.

    The rule of ``wraithguard_toolkit.flatten_dict`` (TES3 Conflict Solver's), so a
    path here is the path the conflict viewer and the queue use.

    Args:
        record: The record.
        parent: The path so far.

    Returns:
        Path -> value.
    """
    out: dict[str, Any] = {}
    for k, v in record.items():
        path = f"{parent}.{k}" if parent else str(k)
        if isinstance(v, dict):
            out.update(_flatten(v, path))
        else:
            out[path] = v
    return out


def _native_read(path: Path, tag: str) -> list[dict[str, Any]]:
    """A plugin's records of one tag, through the Rust backend.

    Args:
        path: The plugin.
        tag: The four-letter tag.

    Returns:
        The records.
    """
    import wraithguard_native

    return list(wraithguard_native.plugin_records(path.read_bytes(), keep=[tag.encode("ascii")]))


def coerce(current: object, raw: object, kind: str | None) -> object:
    """A value the viewer sent, as the type the field holds.

    Args:
        current: The field's value in the winning record (its type is the field's).
        raw: What the viewer sent (already JSON-decoded).
        kind: The schema's kind for the field (:func:`.fieldtypes.field_kind`).

    Returns:
        The value to write.

    Raises:
        EditorError: When it cannot be that type, or is out of the field's range.
    """
    if isinstance(current, bool):
        if isinstance(raw, bool):
            return raw
        if str(raw).strip().lower() in ("true", "1", "yes"):
            return True
        if str(raw).strip().lower() in ("false", "0", "no"):
            return False
        raise EditorError(f"{raw!r} is not true or false")
    if isinstance(current, int):
        try:
            number = float(str(raw).strip())
        except ValueError:
            raise EditorError(f"{raw!r} is not a whole number") from None
        if not number.is_integer():
            raise EditorError(f"{raw!r} is not a whole number")
        value = int(number)
        bounds = int_bounds(kind) if kind else None
        if bounds and not bounds[0] <= value <= bounds[1]:
            raise EditorError(f"{value} is outside {bounds[0]}..{bounds[1]}")
        return value
    if isinstance(current, float):
        try:
            return float(str(raw).strip())
        except ValueError:
            raise EditorError(f"{raw!r} is not a number") from None
    if isinstance(current, (list, dict)):
        if not isinstance(raw, type(current)):
            raise EditorError(f"this field holds a {type(current).__name__}, not {raw!r}")
        return raw
    if isinstance(current, str):
        return raw if isinstance(raw, str) else str(raw)
    return raw


class EditorSession:
    """The editor's records and edits, over Wraithguard's patch queue."""

    def __init__(
        self,
        plugins: Sequence[tuple[str, Path]],
        queue: PatchQueue,
        journal: Path | None = None,
        read: Callable[[Path, str], list[dict[str, Any]]] = _native_read,
    ) -> None:
        """Start a session.

        Args:
            plugins: The load order: ``(file name, path)``.
            queue: The patch queue the edits go into.
            journal: Where the queue is saved after every change (None: nowhere).
            read: Reads one plugin's records of one tag.
        """
        self.order = [name for name, _ in plugins]
        self._paths = {name.lower(): (name, path) for name, path in plugins}
        self._rank = {name.lower(): i for i, name in enumerate(self.order)}
        self.queue = queue
        self.journal = journal
        self._read = read
        self._cache: dict[tuple[str, str], dict[str, dict[str, Any]]] = {}
        self._lock = threading.Lock()

    # -- reading ----------------------------------------------------------------------

    def _records(self, plugin: str, tag: str) -> dict[str, dict[str, Any]]:
        """One plugin's records of a tag, by lower-case key (read once).

        Args:
            plugin: The plugin's name.
            tag: The tag.

        Returns:
            Key -> record.
        """
        low = plugin.lower()
        with self._lock:
            hit = self._cache.get((low, tag))
        if hit is not None:
            return hit
        name_path = self._paths.get(low)
        found: dict[str, dict[str, Any]] = {}
        if name_path is not None:
            try:
                for rec in self._read(name_path[1], tag):
                    key = record_key(rec)
                    if key:
                        found[key.lower()] = rec
            except (OSError, ValueError) as exc:
                LOG.warning("editor: cannot read %s: %s", name_path[1], exc)
        with self._lock:
            self._cache[(low, tag)] = found
        return found

    def find(self, tag: str, rid: str, plugins: Sequence[str] | None = None) -> Found | None:
        """A record, from every plugin that defines it.

        Args:
            tag: Its tag (``NPC_``).
            rid: Its id (any case), or ``(x, y)`` for an exterior cell.
            plugins: The plugins that define it, when the caller knows (the viewer
                does); else every plugin is looked in.

        Returns:
            The record, or None when no plugin has it.
        """
        tag = tag.upper().ljust(4, "_")
        record_type = TAG_TO_TYPE.get(tag)
        if record_type is None:
            return None
        names = [p for p in (plugins or self.order) if p.lower() in self._rank]
        names.sort(key=lambda p: self._rank[p.lower()])
        want = rid.strip().lower()
        versions: list[tuple[str, dict[str, Any]]] = []
        for name in names:
            rec = self._records(name, tag).get(want)
            if rec is not None and rec.get("type") == record_type:
                versions.append((self._paths[name.lower()][0], rec))
        if not versions:
            return None
        return Found(tag, record_type, record_key(versions[-1][1]), tuple(versions))

    # -- the dialog -------------------------------------------------------------------

    def _choices(self, found: Found) -> dict[str, FieldChoice | FieldValue]:
        """The queued choices for a record, by path."""
        return {c.path: c for c in self.queue.fields.get((found.record_type, found.key), [])}

    def view(self, found: Found) -> dict[str, Any]:
        """What the record dialog shows.

        Args:
            found: The record.

        Returns:
            ``{"tag", "type", "id", "winner", "plugins", "whole", "fields"}``; each
            field ``{"path", "value", "queued", "source", "editable", "kind",
            "options"}``, ``queued`` being what the patch would write (absent when
            nothing is queued) and ``source`` the plugin it comes from (or "typed").
        """
        flat = _flatten(found.record)
        choices = self._choices(found)
        by_plugin = {p: _flatten(r) for p, r in found.versions}
        fields = []
        for path, value in flat.items():
            kind = field_kind(found.record_type, path)
            options: list[str] = list(enum_options(path))
            if kind and flags_name(kind):
                options = list(flag_options(flags_name(kind) or ""))
            item: dict[str, Any] = {
                "path": path,
                "value": value,
                "editable": path not in IDENTITY
                and path not in READ_ONLY
                and not path.startswith("references"),
                "kind": kind or type(value).__name__,
                "options": options,
            }
            if isinstance(value, list):
                item["count"] = len(value)
            choice = choices.get(path)
            if isinstance(choice, FieldValue):
                item["queued"], item["source"] = choice.value, "typed"
            elif isinstance(choice, FieldChoice):
                item["queued"], item["source"] = (
                    by_plugin.get(choice.plugin, {}).get(path),
                    choice.plugin,
                )
            fields.append(item)
        whole = next(
            (
                s.plugin
                for s in self.queue.selections
                if (s.record_type, s.key) == (found.record_type, found.key)
            ),
            None,
        )
        return {
            "tag": found.tag,
            "type": found.record_type,
            "id": found.key,
            "winner": found.winner,
            "plugins": [p for p, _ in found.versions],
            "whole": whole,
            "fields": fields,
        }

    def set_field(self, found: Found, path: str, raw: object) -> None:
        """Queue a typed value for one field (and save the journal).

        Args:
            found: The record.
            path: The field's dotted path.
            raw: The value, as the viewer sent it.

        Raises:
            EditorError: For an identity or read-only field, one the record does not
                have, or a value of the wrong type.
        """
        if path in IDENTITY or path in READ_ONLY or path.startswith("references"):
            raise EditorError(f"{path} cannot be changed here")
        current, present = value_at(found.record, path)
        if not present:
            raise EditorError(f"{found.key} has no field {path}")
        value = coerce(current, raw, field_kind(found.record_type, path))
        if value == current:
            self.queue.remove_field(found.record_type, found.key, path)
        else:
            self.queue.add_field(found.record_type, found.key, FieldValue(path=path, value=value))
            self.queue.set_base(found.record_type, found.key, found.winner)
        self.save_journal()

    def revert(self, found: Found, path: str | None = None) -> None:
        """Drop the queued change to one field, or to the whole record.

        Args:
            found: The record.
            path: The field, or None for every queued change to the record.
        """
        if path is None:
            self.queue.remove_record(found.record_type, found.key)
        else:
            self.queue.remove_field(found.record_type, found.key, path)
        self.save_journal()

    def pending(self) -> list[dict[str, Any]]:
        """Everything the patch would carry, editor changes and conflict choices.

        Returns:
            One ``{"tag", "type", "id", "whole", "changes"}`` per record; each change
            ``{"path", "value"}`` (typed) or ``{"path", "plugin"}`` (taken from one).
        """
        out: list[dict[str, Any]] = [
            {
                "tag": tag_of(s.record_type),
                "type": s.record_type,
                "id": s.key,
                "whole": s.plugin,
                "changes": [],
            }
            for s in self.queue.selections
        ]
        for (record_type, key), choices in self.queue.fields.items():
            changes = [
                (
                    {"path": c.path, "value": c.value}
                    if isinstance(c, FieldValue)
                    else {"path": c.path, "plugin": c.plugin}
                )
                for c in choices
            ]
            out.append(
                {
                    "tag": tag_of(record_type),
                    "type": record_type,
                    "id": key,
                    "whole": None,
                    "changes": changes,
                }
            )
        return out

    # -- the journal ------------------------------------------------------------------

    def save_journal(self) -> None:
        """Write the whole queue to the journal (see :func:`save_queue`)."""
        if self.journal is not None:
            save_queue(self.queue, self.journal)

    def forget(self) -> None:
        """Remove the journal (the queue was written, or emptied)."""
        if self.journal is not None:
            forget_queue(self.journal)

    def restore(self) -> int:
        """Put the journal's decisions back into the queue (see :func:`restore_queue`).

        Returns:
            How many records it brought back.
        """
        return restore_queue(self.queue, self.journal) if self.journal is not None else 0


def save_queue(queue: PatchQueue, journal: Path) -> None:
    """Write a whole patch queue to a journal file (atomically); remove it when empty.

    Every decision - whole records, fields taken from a plugin, typed values, and the
    bases the editor gave - so the pool survives a viewer that runs out of memory or a
    Wraithguard that crashes.

    Args:
        queue: The queue.
        journal: The file.
    """
    if not len(queue):
        forget_queue(journal)
        return
    doc = {
        "version": _JOURNAL_VERSION,
        "whole": [
            {"type": s.record_type, "key": s.key, "plugin": s.plugin} for s in queue.selections
        ],
        "fields": [
            {
                "type": record_type,
                "key": key,
                "base": queue.base(record_type, key),
                "choices": [
                    (
                        {"path": c.path, "value": c.value}
                        if isinstance(c, FieldValue)
                        else {"path": c.path, "plugin": c.plugin}
                    )
                    for c in choices
                ],
            }
            for (record_type, key), choices in queue.fields.items()
        ],
        "refs": [
            {
                "cell": e.cell,
                "origin": e.origin,
                "refr": e.refr_index,
                "changes": dict(e.changes),
                "plugins": list(e.plugins),
            }
            for e in queue.ref_edits
        ],
    }
    tmp = journal.with_name(journal.name + ".tmp")
    try:
        tmp.write_text(json.dumps(doc), encoding="utf-8")
        tmp.replace(journal)
    except (OSError, TypeError, ValueError) as exc:
        LOG.warning("patch journal: cannot write %s: %s", journal, exc)


def forget_queue(journal: Path) -> None:
    """Remove a journal file.

    Args:
        journal: The file.
    """
    try:
        journal.unlink(missing_ok=True)
    except OSError as exc:
        LOG.warning("patch journal: cannot remove %s: %s", journal, exc)


def restore_queue(queue: PatchQueue, journal: Path) -> int:
    """Put a journal's decisions back into a queue; what the queue holds already wins.

    Args:
        queue: The queue.
        journal: The file.

    Returns:
        How many records it brought back (0 without a journal, or one that does not
        read).
    """
    if not journal.is_file():
        return 0
    try:
        doc = json.loads(journal.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        LOG.warning("patch journal: cannot read %s: %s", journal, exc)
        return 0
    if not isinstance(doc, dict) or doc.get("version") != _JOURNAL_VERSION:
        return 0
    known = {(s.record_type, s.key) for s in queue.selections} | set(queue.fields)
    n = 0
    try:
        for w in doc.get("whole") or []:
            if (w["type"], w["key"]) not in known:
                queue.add_whole(Selection(plugin=w["plugin"], record_type=w["type"], key=w["key"]))
                n += 1
        for f in doc.get("fields") or []:
            if (f["type"], f["key"]) in known:
                continue
            for c in f.get("choices") or []:
                choice = (
                    FieldValue(c["path"], c["value"])
                    if "value" in c
                    else FieldChoice(c["path"], c["plugin"])
                )
                queue.add_field(f["type"], f["key"], choice)
            if f.get("base"):
                queue.set_base(f["type"], f["key"], f["base"])
            n += 1
        have = {e.ident for e in queue.ref_edits}
        for r in doc.get("refs") or []:
            edit = RefEdit(
                r["cell"],
                r["origin"],
                int(r["refr"]),
                dict(r["changes"]),
                tuple(r.get("plugins") or ()),
            )
            if edit.ident not in have:
                queue.add_ref_edit(edit)
                n += 1
    except (KeyError, TypeError, ValueError) as exc:
        LOG.warning("patch journal: %s is damaged (%s); restored what read", journal, exc)
    return n


def _listing(folder: Path) -> dict[str, Path]:
    """A folder's files by lower-case name (empty when it cannot be read).

    Args:
        folder: The folder.

    Returns:
        Name -> path.
    """
    try:
        return {p.name.lower(): p for p in folder.iterdir() if p.is_file()}
    except OSError:
        return {}


def plugins_from_cfg(cfg: Path) -> list[tuple[str, Path]]:
    """A setup's load order with each plugin's file: what :class:`EditorSession` reads.

    The ``content=`` plugins of an openmw.cfg, each found in the last ``data=`` folder
    that has it (names compared without case, as OpenMW does). ``.omwscripts`` and
    plugins no folder has are left out.

    Args:
        cfg: The openmw.cfg (the viewer's setup).

    Returns:
        ``(name, path)`` in load order.
    """
    from wraithguard.lua.scan import read_cfg_lua

    dirs, content = read_cfg_lua(cfg)
    listings = [_listing(d) for d in dirs]
    out: list[tuple[str, Path]] = []
    for name in content:
        if not name.lower().endswith((".esm", ".esp", ".omwaddon", ".omwgame")):
            continue
        hit = next((lst[name.lower()] for lst in reversed(listings) if name.lower() in lst), None)
        if hit is not None:
            out.append((name, hit))
    return out


__all__ = [
    "READ_ONLY",
    "TAG_TO_TYPE",
    "EditorError",
    "EditorSession",
    "Found",
    "coerce",
    "forget_queue",
    "plugins_from_cfg",
    "restore_queue",
    "save_queue",
    "tag_of",
]
