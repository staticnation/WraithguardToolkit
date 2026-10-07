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

import contextlib
import copy
import json
import re
import threading
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Any, Final

from wraithguard.logging_setup import get_logger
from wraithguard.mwscript.rename import replace_id
from wraithguard.patch.enums import enum_options
from wraithguard.patch.fieldtypes import field_kind, flag_options, flags_name, int_bounds
from wraithguard.patch.merge import IDENTITY, FieldChoice, FieldValue, set_at, value_at
from wraithguard.patch.records import NewRecord, Selection, master_names, record_key
from wraithguard.patch.refedit import REF_FIELDS, NewRef, RefEdit, refs_naming, winning_reference
from wraithguard.tes3fields.naming import TYPE_TO_TAG

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator, Mapping, Sequence
    from pathlib import Path

    from wraithguard.mwscript.compiler import ScriptCompiler
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

#: Fields that hold script source, by record type: a script's text, a response's result.
#: Search & Replace changes an ID inside them as the compiler reads one, not the whole value.
SCRIPT_TEXT_FIELDS: Final[frozenset[tuple[str, str]]] = frozenset(
    {("Script", "text"), ("DialogueInfo", "script_text")}
)

_JOURNAL_VERSION: Final = 1

#: Types "Make a copy as" and "New" refuse: keyed by something other than an id the copy
#: could change (cells, lands, paths, dialogue, which also has an order), or fixed by the
#: engine (skills, magic effects, game settings, land texture indices). A script names
#: itself inside (its `begin` line), which both take care of.
UNCOPYABLE: Final[frozenset[str]] = frozenset(
    {
        "Header",
        "Cell",
        "Landscape",
        "PathGrid",
        "Dialogue",
        "DialogueInfo",
        "Skill",
        "MagicEffect",
        "GameSetting",
        "LandscapeTexture",
        "Script",
        "StartScript",
    }
)

#: The longest id the game reads (32 bytes with the terminator).
MAX_ID: Final = 31

#: The name a record the patch makes is "defined in", in the dialog.
PATCH: Final = "(this patch)"


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
        new: The patch makes it (:class:`.records.NewRecord`): no plugin defines it,
            and its one version is the patch's.
    """

    tag: str
    record_type: str
    key: str
    versions: tuple[tuple[str, dict[str, Any]], ...]
    new: bool = False

    @property
    def winner(self) -> str:
        """The plugin whose version the game uses."""
        return self.versions[-1][0]

    @property
    def record(self) -> dict[str, Any]:
        """The winning version."""
        return self.versions[-1][1]


#: What each changeable field of a reference holds, in the order the reference dialog
#: shows them: ``vec3`` three numbers (rotation in radians), ``grid`` an exterior's
#: ``[x, y]``, ``door`` a teleport door's destination ``{translation, rotation, cell}``
#: (an empty cell is the exterior; None makes it an ordinary door).
REF_KINDS: Final[dict[str, str]] = {
    "translation": "vec3",
    "rotation": "vec3",
    "scale": "float",
    "deleted": "bool",
    "owner": "str",
    "owner_global": "str",
    "owner_faction": "str",
    "owner_faction_rank": "int",
    "lock_level": "int",
    "key": "str",
    "trap": "str",
    "soul": "str",
    "charge_left": "int",
    "health_left": "int",
    "object_count": "int",
    "blocked": "int",
    "temporary": "bool",
    "moved_cell": "grid",
    "destination": "door",
}

#: An exterior cell's side, in units.
CELL_SIZE: Final = 8192


def exterior_grid(cell: str) -> tuple[int, int] | None:
    """An exterior cell key's grid (``"(x, y)"`` -> ``(x, y)``), or None for an interior."""
    import re

    m = re.fullmatch(r"\((-?\d+), (-?\d+)\)", cell.strip())
    return (int(m.group(1)), int(m.group(2))) if m else None


def grid_of(position: Sequence[float]) -> tuple[int, int]:
    """The exterior cell a world position is in."""
    import math

    return (math.floor(position[0] / CELL_SIZE), math.floor(position[1] / CELL_SIZE))


#: The scale the game allows a reference (the Construction Set's limits).
_SCALE: Final = (0.5, 2.0)
#: A signed 32-bit field's range.
_I32: Final = (-(2**31), 2**31 - 1)


def _whole(raw: object, what: str) -> int:
    """A whole number from what the viewer sent."""
    try:
        number = float(str(raw).strip())
    except ValueError:
        raise EditorError(f"{raw!r} is not a whole number ({what})") from None
    if not number.is_integer():
        raise EditorError(f"{raw!r} is not a whole number ({what})")
    return int(number)


def _vec3(raw: object, what: str) -> list[float]:
    """Three finite numbers from what the viewer sent."""
    import math

    if not isinstance(raw, list) or len(raw) != 3:
        raise EditorError(f"{what} is three numbers")
    try:
        out = [float(v) for v in raw]
    except (TypeError, ValueError):
        raise EditorError(f"{what} is three numbers") from None
    if not all(math.isfinite(v) for v in out):
        raise EditorError(f"{what} must be finite")
    return out


def same_value(a: object, b: object) -> bool:
    """Whether two field values are the same, floats to within a millionth."""
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        if isinstance(a, bool) or isinstance(b, bool):
            return a is b
        return abs(float(a) - float(b)) < 1e-6
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(same_value(x, y) for x, y in zip(a, b, strict=True))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(same_value(a[k], b[k]) for k in a)
    return a == b


def coerce_ref(name: str, raw: object) -> object:
    """A value the viewer sent for a reference's field, as the field holds it.

    An empty value clears an optional field (``None``: the reference does not have it).

    Args:
        name: The field (:data:`REF_KINDS`).
        raw: What the viewer sent (JSON-decoded).

    Returns:
        The value to write.

    Raises:
        EditorError: For a field that is not changed here, or a value it cannot hold.
    """
    kind = REF_KINDS.get(name)
    if kind is None or name not in REF_FIELDS:
        raise EditorError(f"{name} cannot be changed on a reference here")
    if kind == "vec3":
        return _vec3(raw, name)
    if kind == "door":
        if raw is None or raw == "":
            return None
        if not isinstance(raw, dict):
            raise EditorError("a destination is {translation, rotation, cell}")
        cell = raw.get("cell", "")
        if not isinstance(cell, str):
            raise EditorError("a destination's cell is a name (empty: the exterior)")
        return {
            "translation": _vec3(raw.get("translation"), "the destination's position"),
            "rotation": _vec3(raw.get("rotation", [0.0, 0.0, 0.0]), "the destination's rotation"),
            "cell": cell.strip(),
        }
    if kind == "bool":
        if isinstance(raw, bool):
            return raw
        text = str(raw).strip().lower()
        if text in ("true", "1", "yes"):
            return True
        if text in ("false", "0", "no", ""):
            return False
        raise EditorError(f"{raw!r} is not true or false")
    if raw is None or (isinstance(raw, str) and not raw.strip()):
        return None
    if kind == "str":
        return str(raw)
    if kind == "grid":
        if not isinstance(raw, list) or len(raw) != 2:
            raise EditorError("moved_cell is an exterior's grid, [x, y]")
        return [_whole(raw[0], name), _whole(raw[1], name)]
    if kind == "float":
        try:
            value = float(str(raw).strip())
        except ValueError:
            raise EditorError(f"{raw!r} is not a number") from None
        if name == "scale" and not _SCALE[0] <= value <= _SCALE[1]:
            raise EditorError(f"scale is {_SCALE[0]} to {_SCALE[1]}")
        return value
    value = _whole(raw, name)
    if not _I32[0] <= value <= _I32[1]:
        raise EditorError(f"{value} is out of range")
    return value


@dataclass(frozen=True)
class FoundRef:
    """One placed object, as the load order resolves it.

    Attributes:
        cell: Its cell's key (an interior's name, an exterior's ``"(x, y)"``).
        origin: The plugin that created it, spelled as the load order spells it.
        refr_index: Its index in that plugin.
        plugins: The plugins with a CELL record for the cell, in load order.
        winner: The plugin whose version of the reference the game uses.
        ref: That version.
    """

    cell: str
    origin: str
    refr_index: int
    plugins: tuple[str, ...]
    winner: str
    ref: dict[str, Any]


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


def _pending_diff(before: list[dict[str, Any]], after: list[dict[str, Any]]) -> list[str]:
    """What differs between two :meth:`EditorSession.pending` lists, in words.

    Args:
        before: The pool's entries before.
        after: And after.

    Returns:
        The names of the records and references whose entry changed, came or went.
    """

    def name(p: dict[str, Any]) -> str:
        """An entry's name: a record's tag and id, or a reference's object and key."""
        if p.get("ref"):
            r = p["ref"]
            return f"{p.get('id', '')} ({r.get('origin', '')}:{r.get('refr', '')})"
        if p.get("new"):
            return f"new {p['new'].get('id', p.get('id', ''))}"
        return f"{p.get('tag') or p.get('type', '')} {p.get('id', '')}"

    def keyed(items: list[dict[str, Any]]) -> dict[str, str]:
        """Each entry by its name, as comparable text."""
        return {name(p): json.dumps(p, sort_keys=True, default=str) for p in items}

    a, b = keyed(before), keyed(after)
    return sorted(k for k in a.keys() | b.keys() if a.get(k) != b.get(k))


def _split_query(query: str) -> list[str]:
    """A search's words: spaces part them, except inside double quotes."""
    import shlex

    try:
        return [w for w in shlex.split(query) if w.strip()]
    except ValueError:  # an unclosed quote: the words as they are
        return [w for w in query.split() if w.strip()]


_TERM = re.compile(r"^([A-Za-z_][\w.]*)(!=|>=|<=|=|>|<|:)(.*)$")


def _parse_term(word: str) -> tuple[str, str, str]:
    """``(path, operator, value)`` for one word; a bare word is ``("", ":", word)``."""
    m = _TERM.match(word)
    if m:
        return m.group(1).lower(), m.group(2), m.group(3)
    return "", ":", word


def _as_text(value: object) -> str:
    """A value as the search reads it."""
    if isinstance(value, (list, tuple)):
        return " ".join(_as_text(v) for v in value)
    if isinstance(value, dict):
        return " ".join(_as_text(v) for v in value.values())
    if isinstance(value, bytes):
        return ""
    return "" if value is None else str(value)


def _match_term(term: tuple[str, str, str], flat: Mapping[str, Any]) -> str | None:
    """The first field of a flattened record a search word matches.

    Args:
        term: The word (:func:`_parse_term`).
        flat: The record, flattened.

    Returns:
        ``path: value``, or None when no field matches.
    """
    path, op, want = term
    low = want.lower()
    for k, v in flat.items():
        if path and path not in k.lower():
            continue
        text = _as_text(v)
        hit = False
        if op == ":":
            hit = low in text.lower() if low else text != ""
        elif op in ("=", "!="):
            hit = (text.lower() == low) == (op == "=")
        else:
            try:
                a, b = float(text), float(want)
            except ValueError:
                continue
            hit = {">": a > b, "<": a < b, ">=": a >= b, "<=": a <= b}[op]
        if hit:
            shown = text if len(text) <= 80 else text[:79] + "..."
            return f"{k}: {shown}"
    return None


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


def _native_read_all(path: Path) -> list[dict[str, Any]]:
    """All of a plugin's records, through the Rust backend.

    Args:
        path: The plugin.

    Returns:
        The records.
    """
    import wraithguard_native

    return list(wraithguard_native.plugin_records(path.read_bytes()))


def _template_for(current: list[Any], entry: object) -> object:
    """The existing entry a new one is checked against.

    One with the same fields, for a list whose entries differ in kind (AI packages: a
    wander, a travel...), else the first.
    """
    if isinstance(entry, dict):
        keys = set(entry)
        for have in current:
            if isinstance(have, dict) and set(have) == keys:
                return have
    return current[0]


def _coerce_entry(template: object, entry: object) -> object:
    """One entry of a list, checked by the shape of the first.

    A row (``[count, id]``) position by position, anything else as :func:`coerce` takes it.
    """
    if isinstance(template, list):
        if not isinstance(entry, list) or len(entry) != len(template):
            raise EditorError(f"an entry here is {len(template)} values, not {entry!r}")
        return [coerce(t, v, None) for t, v in zip(template, entry, strict=True)]
    return coerce(template, entry, None)


def _native_read_dialogue(path: Path) -> list[dict[str, Any]]:
    """A plugin's topics and responses, in file order, through the Rust backend.

    File order matters: a response belongs to the topic before it.

    Args:
        path: The plugin.

    Returns:
        The DIAL and INFO records.
    """
    import wraithguard_native

    return list(wraithguard_native.plugin_records(path.read_bytes(), keep=[b"DIAL", b"INFO"]))


def coerce(current: object, raw: object, kind: str | None) -> object:
    """A value the viewer sent, as the type the field holds.

    Args:
        current: The field's value in the winning record (its type is the field's).
        raw: What the viewer sent (already JSON-decoded).
        kind: The schema's kind for the field (:func:`.fieldtypes.field_kind`).

    Returns:
        The value to write. A list is checked entry by entry against the shape of its
        first entry (an inventory's ``[count, id]``, a leveled list's ``[id, level]``, an
        AI package's fields); a group against its own fields.

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
    if isinstance(current, list):
        if not isinstance(raw, list):
            raise EditorError(f"this field holds a list, not {raw!r}")
        if not current:
            return raw  # nothing to say what an entry is
        return [_coerce_entry(_template_for(current, entry), entry) for entry in raw]
    if isinstance(current, dict):
        if not isinstance(raw, dict):
            raise EditorError(f"this field holds a group, not {raw!r}")
        unknown = set(raw) - set(current)
        if unknown:
            raise EditorError(f"{', '.join(sorted(unknown))} is not part of this entry")
        return {k: coerce(current[k], raw.get(k, current[k]), None) for k in current}
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
        read_all: Callable[[Path], list[dict[str, Any]]] = _native_read_all,
        read_dialogue: Callable[[Path], list[dict[str, Any]]] = _native_read_dialogue,
    ) -> None:
        """Start a session.

        Args:
            plugins: The load order: ``(file name, path)``.
            queue: The patch queue the edits go into.
            journal: Where the queue is saved after every change (None: nowhere).
            read: Reads one plugin's records of one tag.
            read_all: Reads all of one plugin's records (the Use Report).
            read_dialogue: Reads one plugin's topics and responses, in file order.
        """
        self.order = [name for name, _ in plugins]
        self._paths = {name.lower(): (name, path) for name, path in plugins}
        self._rank = {name.lower(): i for i, name in enumerate(self.order)}
        self.queue = queue
        self.journal = journal
        self._read = read
        self._read_all = read_all
        self._read_dialogue = read_dialogue
        self._dialogue_cache: dict[str, list[dict[str, Any]]] = {}
        self._cache: dict[tuple[str, str], dict[str, dict[str, Any]]] = {}
        self._masters: dict[str, list[str]] = {}
        self._lock = threading.Lock()
        self._script_compiler: tuple[str, ScriptCompiler] | None = None
        # The undo history starts from the pool as the session finds it.
        self._history: dict[str, Any] = {"undo": [], "redo": [], "last": queue.snapshot()}

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
            made = self.queue.new_record(record_type, rid.strip())
            if made is None:
                return None
            return Found(tag, record_type, made.key, ((PATCH, dict(made.record)),), new=True)
        return Found(tag, record_type, record_key(versions[-1][1]), tuple(versions))

    def search(self, tag: str, query: str, limit: int = 20000) -> dict[str, Any]:
        """The Object Window's search by any field (CSSE's filter).

        The records of a type whose fields match, as the load order resolves them (the
        pool's typed values and the patch's own records included).

        The query is words, each of which must match (all of them):

        - ``text`` - any field's value contains it (any case);
        - ``path:text`` - a field whose path contains ``path`` has a value containing it
          (``script:``, ``data.weight:5``; ``path:`` alone - the field is set);
        - ``path=value`` - equal (as text, any case), ``path!=value`` - not equal;
        - ``path>n``, ``path<n``, ``path>=n``, ``path<=n`` - a number compared.

        Lists are matched by their entries (an inventory's ``[count, id]``, a spell list).

        Args:
            tag: The tab's tag (``WEAP``).
            query: The words.
            limit: The most records answered.

        Returns:
            ``{"hits": {id (lower case): "path: value" (the first field that matched)},
            "count": how many matched, "capped": whether it stopped at ``limit``}``.

        Raises:
            EditorError: For a tag the editor does not know.
        """
        tag = tag.upper().ljust(4, "_")[:4]
        record_type = TAG_TO_TYPE.get(tag)
        if record_type is None:
            raise EditorError(f"{tag} is not a record type")
        terms = [_parse_term(t) for t in _split_query(query)]
        if not terms:
            return {"hits": {}, "count": 0, "capped": False}
        winners: dict[str, dict[str, Any]] = {}
        for name in self.order:
            winners.update(
                {k: r for k, r in self._records(name, tag).items() if r.get("type") == record_type}
            )
        for (rtype, key), choices in self.queue.fields.items():
            if rtype != record_type or key.lower() not in winners:
                continue
            rec = copy.deepcopy(winners[key.lower()])
            for c in choices:
                if isinstance(c, FieldValue):
                    set_at(rec, c.path, c.value)
            winners[key.lower()] = rec
        for made in self.queue.new_records:
            if made.record_type == record_type:
                winners[made.key.lower()] = dict(made.record)
        hits: dict[str, str] = {}
        count = 0
        for key, rec in winners.items():
            flat = _flatten(rec)
            first = ""
            for term in terms:
                got = _match_term(term, flat)
                if got is None:
                    break
                first = first or got
            else:
                count += 1
                if len(hits) < limit:
                    hits[key] = first
        return {"hits": hits, "count": count, "capped": count > limit}

    def uses(self, found: Found) -> dict[str, Any]:
        """The record's Use Report: every record of the load order that names it.

        The Rust backend scans the plugins (only those whose bytes hold the id are
        parsed); it runs where :meth:`find` does, off the queue's thread.

        Args:
            found: The record.

        Returns:
            ``{"tag", "id", "uses": [{"plugin", "type", "tag", "key", "paths", "count",
            "wins"}], "live", "cells"}`` - ``live`` the uses in versions the load order
            uses, ``cells`` the references placed.
        """
        from wraithguard.patch.uses import native_use_report, use_report

        plugins = [self._paths[n.lower()] for n in self.order]
        found_uses = (
            native_use_report(plugins, found.record_type, found.key)
            if self._read_all is _native_read_all
            else None
        )
        if found_uses is None:
            found_uses = use_report(plugins, self._read_all, found.record_type, found.key)
        rows = [
            {
                "plugin": u.plugin,
                "type": u.record_type,
                "tag": tag_of(u.record_type),
                "key": u.key,
                "paths": u.paths,
                "count": u.count,
                "wins": u.wins,
            }
            for u in found_uses
        ]
        return {
            "tag": found.tag,
            "id": found.key,
            "uses": rows,
            "live": sum(u.count for u in found_uses if u.wins),
            "cells": sum(u.count for u in found_uses if u.record_type == "Cell"),
        }

    @staticmethod
    def _field_of(record: Mapping[str, Any], path: str) -> str:
        """The dialog's field that holds a dotted path.

        Up to the first list: lists are one field, as the conflict viewer keeps them.
        """
        parts = path.split(".")
        current: Any = record
        for i, part in enumerate(parts):
            if isinstance(current, list):
                return ".".join(parts[:i])
            if not isinstance(current, dict) or part not in current:
                return ".".join(parts[: i + 1])
            current = current[part]
        return path

    @staticmethod
    def _swap(value: object, old: str, new: str) -> object:
        """A copy of a value with every string equal to ``old`` (any case) made ``new``."""
        if isinstance(value, str):
            return new if value.lower() == old.lower() else value
        if isinstance(value, list):
            return [EditorSession._swap(v, old, new) for v in value]
        if isinstance(value, dict):
            return {k: EditorSession._swap(v, old, new) for k, v in value.items()}
        return value

    def replace_plan(
        self, found: Found, new_id: str, cell: str | None = None
    ) -> tuple[list[tuple[Found, str, object] | RefEdit | NewRef], dict[str, Any]]:
        """What Search & Replace would change: each live use's field, with ``new_id``.

        Reads the load order (:meth:`uses`); runs off the queue's thread.

        Args:
            found: The record whose uses are repointed.
            new_id: The record they should name instead (of the same type).
            cell: Only the references placed in this cell (its key: an interior's name,
                an exterior's ``"(x, y)"``) - "Replace all in the current cell"; no
                record's fields change. None: every live use.

        Returns:
            ``(plan, report)``: ``(using record, field, new value)`` per field and a
            :class:`.refedit.RefEdit` per placed reference (its key kept, the object
            changed - the engine merges it so), and the Use Report it was made from
            (:meth:`uses`). Scripts' text and dialogue results are changed where the ID
            stands as a word (:func:`~wraithguard.mwscript.rename.replace_id`); a script
            is recompiled when it is queued (:meth:`replace_uses`), so its bytecode names
            the new ID too.

        Raises:
            EditorError: When ``new_id`` is not a record of the same type, or is the same.
        """
        target = self.find(found.tag, new_id)
        if target is None:
            raise EditorError(f"{new_id} is not a {found.record_type} of this load order")
        if target.key.lower() == found.key.lower():
            raise EditorError("that is the same record")
        report = self.uses(found)
        plan: list[tuple[Found, str, object] | RefEdit | NewRef] = []
        only = cell.strip().lower() if cell else None
        for use in report["uses"]:
            if use["type"] == "Cell" or only is not None:
                continue
            if not use["wins"] or not use["tag"]:
                continue
            user = self.find(use["tag"], use["key"])
            if user is None:
                continue
            record = user.record
            for field in dict.fromkeys(self._field_of(record, p) for p in use["paths"]):
                current, present = value_at(record, field)
                if not present:
                    continue
                if (user.record_type, field) in SCRIPT_TEXT_FIELDS and isinstance(current, str):
                    text, n = replace_id(current, found.key, target.key)
                    if n:
                        plan.append((user, field, text))
                    continue
                plan.append((user, field, self._swap(current, found.key, target.key)))
        cells = dict.fromkeys(u["key"] for u in report["uses"] if u["type"] == "Cell")
        for key in cells:
            if only is not None and key.lower() != only:
                continue
            holders = self._cell_plugins(key, None)
            versions = [(p, self._records(p, "CELL")[key.lower()]) for p in holders]
            masters_of = {p: self._masters_of(p) for p in holders}
            for origin, refr in refs_naming(versions, masters_of, found.key):
                plan.append(RefEdit(key, origin, refr, {"id": target.key}, holders))
        # The references the patch itself places become it too.
        for new in self.queue.new_refs:
            if only is not None and new.cell.lower() != only:
                continue
            if str(new.fields.get("id", "")).lower() == found.key.lower():
                plan.append(replace(new, fields={**new.fields, "id": target.key}))
        return plan, report

    def replace_uses(self, plan: Sequence[tuple[Found, str, object] | RefEdit | NewRef]) -> int:
        """Queue a Search & Replace plan (:meth:`replace_plan`), on the queue's thread.

        Args:
            plan: The changes.

        Returns:
            How many records and placed references it changed.
        """
        changed: set[tuple[str, str]] = set()
        refs = 0
        for item in plan:
            if isinstance(item, RefEdit):
                self.queue.add_ref_edit(item)
                refs += 1
                continue
            if isinstance(item, NewRef):
                self.queue.add_new_ref(item)
                refs += 1
                continue
            user, field, value = item
            if user.record_type == "Script" and field == "text" and isinstance(value, str):
                # The text and, when it compiles, its bytecode (set_script_text).
                self.set_script_text(user, value)
                changed.add((user.record_type, user.key))
                continue
            if user.new:
                made = self.queue.new_record(user.record_type, user.key)
                if made is None:
                    continue
                record = copy.deepcopy(dict(made.record))
                set_at(record, field, value)
                self.queue.add_new_record(replace(made, record=record))
            else:
                self.queue.add_field(
                    user.record_type, user.key, FieldValue(path=field, value=value)
                )
                self.queue.set_base(user.record_type, user.key, user.winner)
            changed.add((user.record_type, user.key))
        self.save_journal()
        return len(changed) + refs

    # -- dialogue ---------------------------------------------------------------------

    def _dialogue(self, plugin: str) -> list[dict[str, Any]]:
        """One plugin's topics and responses, in file order (read once)."""
        low = plugin.lower()
        with self._lock:
            hit = self._dialogue_cache.get(low)
        if hit is not None:
            return hit
        found: list[dict[str, Any]] = []
        name_path = self._paths.get(low)
        if name_path is not None:
            try:
                found = self._read_dialogue(name_path[1])
            except (OSError, ValueError) as exc:
                LOG.warning("editor: cannot read %s: %s", name_path[1], exc)
        with self._lock:
            self._dialogue_cache[low] = found
        return found

    def topics(self) -> list[dict[str, Any]]:
        """Every topic of the load order: ``[{"id", "type", "plugins"}]``, by id."""
        out: dict[str, dict[str, Any]] = {}
        for plugin in self.order:
            for rec in self._dialogue(plugin):
                if rec.get("type") != "Dialogue":
                    continue
                tid = str(rec.get("id") or "")
                entry = out.setdefault(tid.lower(), {"id": tid, "type": "", "plugins": []})
                entry["type"] = str(rec.get("dialogue_type") or entry["type"])
                entry["plugins"].append(self._paths[plugin.lower()][0])
        for made in self.queue.new_records:
            if made.record_type == "Dialogue":
                entry = out.setdefault(
                    made.key.lower(), {"id": made.key, "type": "", "plugins": []}
                )
                entry["type"] = str(made.record.get("dialogue_type") or entry["type"])
                entry["plugins"].append(PATCH)
        return sorted(out.values(), key=lambda t: (t["type"], t["id"].lower()))

    def topic(self, topic_id: str) -> dict[str, Any]:
        """A topic's responses in the order the engine reads them.

        Args:
            topic_id: The topic (any case).

        Returns:
            ``{"id", "type", "responses": [{"id", "text", "speaker", "disposition",
            "plugins", "winner", "orphan", "quest"}]}``: ``speaker`` the response's
            conditions in a few words, ``orphan`` a response whose predecessor is not in
            the topic (it goes last, :mod:`.dialogue`), ``quest`` a journal entry's
            ``Name``/``Finished``/``Restart`` (its index is ``disposition``).

        Raises:
            EditorError: For a topic no plugin has.
        """
        from wraithguard.patch.dialogue import Response, orphans, responses_by_topic, topic_order

        want = topic_id.strip().lower()
        defs: list[Any] = []
        latest: dict[str, tuple[str, dict[str, Any]]] = {}
        spelled, kind = "", ""
        for plugin in self.order:
            name = self._paths[plugin.lower()][0]
            records = self._dialogue(plugin)
            for tid, found in responses_by_topic(records, name).items():
                if tid.lower() == want:
                    spelled = spelled or tid
                    defs.extend(found)
            current = ""
            for rec in records:
                if rec.get("type") == "Dialogue":
                    current = str(rec.get("id") or "").lower()
                    if current == want:
                        kind = str(rec.get("dialogue_type") or kind)
                elif rec.get("type") == "DialogueInfo" and current == want:
                    latest[str(rec.get("id") or "")] = (name, rec)
        for made in self.queue.new_records:
            if made.record_type == "Dialogue" and made.key.lower() == want:
                spelled = spelled or made.key
                kind = str(made.record.get("dialogue_type") or kind)
            if made.record_type == "DialogueInfo" and made.topic.lower() == want:
                rec = dict(made.record)
                defs.append(Response(made.key, str(rec.get("prev_id") or ""), PATCH))
                latest[made.key] = (PATCH, rec)
        if not spelled and not kind:
            raise EditorError(f"no plugin of this load order has the topic {topic_id}")
        # A response the pool moves (its prev_id queued): the patch's version, read last.
        for (rtype, key), choices in self.queue.fields.items():
            if rtype != "DialogueInfo" or key not in latest:
                continue
            for c in choices:
                if isinstance(c, FieldValue) and c.path == "prev_id":
                    defs.append(Response(key, str(c.value or ""), PATCH))
                    winner, rec = latest[key]
                    latest[key] = (winner, {**rec, "prev_id": c.value})
        order = topic_order(defs)
        lost = set(orphans(order))
        rows = []
        for placed in order:
            winner, rec = latest.get(placed.key, ("", {}))
            raw_data = rec.get("data")
            data: dict[str, Any] = raw_data if isinstance(raw_data, dict) else {}
            bits = [
                f"{label} {rec[k]}"
                for k, label in (
                    ("speaker_id", "who:"),
                    ("speaker_race", "race:"),
                    ("speaker_class", "class:"),
                    ("speaker_faction", "faction:"),
                    ("speaker_cell", "cell:"),
                    ("player_faction", "player faction:"),
                )
                if rec.get(k)
            ]
            if rec.get("filters"):
                bits.append(f"{len(rec['filters'])} condition(s)")
            rows.append(
                {
                    "id": placed.key,
                    "text": str(rec.get("text") or ""),
                    "speaker": ", ".join(bits),
                    "disposition": data.get("disposition"),
                    "plugins": list(placed.plugins),
                    "winner": winner,
                    "orphan": placed.key in lost,
                    "quest": str(rec.get("quest_state") or ""),
                }
            )
        return {"id": spelled or topic_id, "type": kind, "responses": rows}

    def move_response(self, topic_id: str, info_id: str, after: str = "") -> dict[str, Any]:
        """Move a response to after another (or to the top) - "drag to reorder".

        The engine moves a response it has seen before when a later plugin gives it a
        different ``prev_id``, and the patch loads last; so the move is the response's
        ``prev_id`` (and ``next_id``, naming the one that will follow it, as the
        Construction Set writes it), queued in the pool. Nothing else changes.

        Args:
            topic_id: The topic (any case).
            info_id: The response to move.
            after: The response it should follow, or empty for the top.

        Returns:
            The topic after the move (:meth:`topic`).

        Raises:
            EditorError: For a response or ``after`` not in the topic, or a response
                placed after itself.
        """
        view = self.topic(topic_id)
        ids = [r["id"] for r in view["responses"]]
        if info_id not in ids:
            raise EditorError(f"{info_id} is not a response of {view['id']}")
        if after and after not in ids:
            raise EditorError(f"{after} is not a response of {view['id']}")
        if after == info_id:
            raise EditorError("a response cannot follow itself")
        rest = [i for i in ids if i != info_id]
        at = rest.index(after) + 1 if after else 0
        following = rest[at] if at < len(rest) else ""
        if [*rest[:at], info_id, *rest[at:]] == ids:
            return view  # already there
        made = self.queue.new_record("DialogueInfo", info_id)
        if made is not None:
            record = copy.deepcopy(dict(made.record))
            record["prev_id"], record["next_id"] = after, following
            self.queue.add_new_record(replace(made, record=record))
        else:
            found = self.find("INFO", info_id)
            if found is None:
                raise EditorError(f"{info_id} is not a response of this load order")
            for path, value in (("prev_id", after), ("next_id", following)):
                self.queue.add_field(
                    found.record_type, found.key, FieldValue(path=path, value=value)
                )
            self.queue.set_base(found.record_type, found.key, found.winner)
        self.save_journal()
        return self.topic(topic_id)

    def new_response(self, topic_id: str, after: str = "") -> Found:
        """Add a response to a topic, after another (or at the top), in the patch.

        The engine puts a response it has not seen straight after the one its
        ``prev_id`` names, and the patch loads last, so that is the whole of placing it;
        ``next_id`` names the response that followed, as the Construction Set writes it.
        It is written inside its topic (the winning version of the topic's record).

        Args:
            topic_id: The topic (any case).
            after: The response it follows, or empty for the top of the topic.

        Returns:
            The response, as :meth:`find` finds it (``INFO``, edited in the dialog).

        Raises:
            EditorError: A topic no plugin has, or ``after`` not one of its responses.
        """
        import secrets

        from wraithguard.esp.json import _by_name, record_to_json

        view = self.topic(topic_id)
        ids = [r["id"] for r in view["responses"]]
        if after and after not in ids:
            raise EditorError(f"{after} is not a response of {view['id']}")
        holder = next(
            (t for t in self.topics() if t["id"].lower() == topic_id.strip().lower()), None
        )
        source = holder["plugins"][-1] if holder else ""
        if after:
            at = ids.index(after) + 1
            following = ids[at] if at < len(ids) else ""
        else:
            following = ids[0] if ids else ""
        rid = str(secrets.randbelow(9 * 10**18) + 10**18)
        while rid in ids:
            rid = str(secrets.randbelow(9 * 10**18) + 10**18)
        record = record_to_json(_by_name()["DialogueInfo"]())
        record.update({"id": rid, "prev_id": after, "next_id": following})
        if isinstance(record.get("data"), dict) and view["type"]:
            record["data"]["dialogue_type"] = view["type"]
        self.queue.add_new_record(NewRecord("DialogueInfo", rid, record, source, view["id"]))
        self.save_journal()
        return Found("INFO", "DialogueInfo", rid, ((PATCH, record),), new=True)

    def copy_topic(self, topic_id: str, new_id: str) -> dict[str, Any]:
        """Copy a topic under a new name, with its responses, in the patch.

        The topic's record is the patch's own (its kind kept); each response is the
        version the load order uses, copied in the order the engine reads them, under a
        new id and linked to the copies before and after it, so the new topic reads as
        the old one does.

        Args:
            topic_id: The topic to copy (any case).
            new_id: The new topic's name.

        Returns:
            The new topic, as :meth:`topic` shows it.

        Raises:
            EditorError: A topic no plugin has, or a name a topic already has.
        """
        import secrets

        from wraithguard.esp.json import _by_name, record_to_json

        view = self.topic(topic_id)
        name = new_id.strip()
        if not name:
            raise EditorError("the topic needs a name")
        if any(t["id"].lower() == name.lower() for t in self.topics()):
            raise EditorError(f"{name} is already a topic in this load order")
        # Each response as the load order uses it, and the plugin it came from (a master
        # of the patch: what it names - speakers, scripts - is defined there).
        latest: dict[str, tuple[str, dict[str, Any]]] = {}
        for plugin in self.order:
            current = ""
            name_of = self._paths[plugin.lower()][0]
            for rec in self._dialogue(plugin):
                if rec.get("type") == "Dialogue":
                    current = str(rec.get("id") or "").lower()
                elif rec.get("type") == "DialogueInfo" and current == view["id"].lower():
                    latest[str(rec.get("id") or "")] = (name_of, rec)
        for made in self.queue.new_records:
            if made.record_type == "DialogueInfo" and made.topic.lower() == view["id"].lower():
                latest[made.key] = (made.source, dict(made.record))
        topic = record_to_json(_by_name()["Dialogue"]())
        topic.update({"id": name, "dialogue_type": view["type"] or topic["dialogue_type"]})
        self.queue.add_new_record(NewRecord("Dialogue", name, topic, ""))
        ids = [str(secrets.randbelow(9 * 10**18) + 10**18) for _ in view["responses"]]
        for i, row in enumerate(view["responses"]):
            source, found = latest.get(row["id"], ("", {}))
            rec = copy.deepcopy(found)
            rec.update(
                {
                    "type": "DialogueInfo",
                    "id": ids[i],
                    "prev_id": ids[i - 1] if i else "",
                    "next_id": ids[i + 1] if i + 1 < len(ids) else "",
                }
            )
            flags = str(rec.get("flags") or "")
            rec["flags"] = " | ".join(
                f for f in flags.split(" | ") if f.strip() and f.strip() != "DELETED"
            )
            self.queue.add_new_record(NewRecord("DialogueInfo", ids[i], rec, source, name))
        self.save_journal()
        return self.topic(name)

    # -- scripts ----------------------------------------------------------------------

    def script_globals(self) -> list[str]:
        """Every global variable the load order defines (read once per plugin)."""
        names: set[str] = set()
        for plugin in self.order:
            names.update(self._records(plugin, "GLOB"))
        return sorted(names)

    def script_view(
        self, found: Found, text: str | None = None, compiler: ScriptCompiler | None = None
    ) -> dict[str, Any]:
        """The Script Edit window: the source, its checks, and the compiled listing.

        Args:
            found: The script.
            text: Source to check instead of the record's (the window's, as typed).
            compiler: :meth:`script_compiler`'s, when the caller has it (off the queue's
                thread, it must).

        Returns:
            ``{"id", "text", "findings": [{"line", "level", "message"}], "listing",
            "compiled"}`` - ``compiled`` whether the record carries bytecode, which a
            changed text does not rebuild (OpenMW compiles the text; Morrowind.exe runs
            the bytecode).

        Raises:
            EditorError: For a record that is not a script.
        """
        from wraithguard.mwscript.check import check_script

        if found.record_type != "Script":
            raise EditorError(f"{found.key} is not a script")
        source = text if text is not None else str(found.record.get("text") or "")
        bytecode = found.record.get("bytecode")
        listing = ""
        if bytecode:
            from wraithguard.mwscript.tes3conv import listing_for_bytecode_field

            listing = listing_for_bytecode_field(bytecode, str(found.record.get("text") or ""))
        findings = check_script(source, found.key, self.script_globals())
        result = (compiler or self.script_compiler()).compile(source)
        return {
            "id": found.key,
            "text": source,
            "findings": [
                {"line": f.line, "level": f.level, "message": f.message} for f in findings
            ],
            "listing": listing,
            "compiled": bool(bytecode),
            "compile": {
                "ok": result.ok,
                "bytes": len(result.data),
                "messages": [
                    {"line": m.line, "level": m.level, "message": m.text} for m in result.messages
                ],
            },
        }

    def pool_script_records(
        self,
    ) -> list[tuple[str, str, str, int, list[str], list[str], list[str]]]:
        """What the patch pool adds that a script can name: its own records.

        Reads the queue: call it on the thread that owns it.
        """
        from wraithguard.mwscript.tes3conv import BytecodeDecodeError, decode_variables_field

        out = []
        for made in self.queue.new_records:
            rec = made.record
            tag = TYPE_TO_TAG.get(made.record_type, made.record_type.upper()[:4])
            dial = -1
            if made.record_type == "Dialogue":
                kinds = ("Topic", "Voice", "Greeting", "Persuasion", "Journal")
                kind = str(rec.get("dialogue_type") or "Topic")
                dial = kinds.index(kind) if kind in kinds else 0
            shorts: list[str] = []
            longs: list[str] = []
            floats: list[str] = []
            if made.record_type == "Script" and rec.get("variables"):
                try:
                    names = decode_variables_field(rec["variables"])
                except BytecodeDecodeError:
                    names = []
                head = rec.get("header") or {}
                ns, nl = int(head.get("num_shorts", 0)), int(head.get("num_longs", 0))
                shorts, longs, floats = names[:ns], names[ns : ns + nl], names[ns + nl :]
            out.append((tag, made.key, str(rec.get("script") or ""), dial, shorts, longs, floats))
        return out

    def script_compiler(
        self, extra: list[tuple[str, str, str, int, list[str], list[str], list[str]]] | None = None
    ) -> ScriptCompiler:
        """The load order (and the pool's own records), read once, for compiling scripts.

        Rebuilt when the pool's records change; the plugins are read again only then.

        Args:
            extra: :meth:`pool_script_records`, read on the queue's thread by a caller on
                another; None to read it here (the queue's thread).
        """
        from wraithguard.mwscript.compiler import ScriptCompiler, find_custom_functions

        if extra is None:
            extra = self.pool_script_records()
        key = repr(extra)
        with self._lock:
            cached = self._script_compiler
            if cached is not None and cached[0] == key:
                return cached[1]
        paths = [self._paths[name.lower()][1] for name in self.order]
        compiler = ScriptCompiler(paths, extra, custom=find_custom_functions(paths))
        with self._lock:
            self._script_compiler = (key, compiler)
        return compiler

    def result_check(
        self, text: str, speaker: str = "", compiler: ScriptCompiler | None = None
    ) -> dict[str, Any]:
        """A dialogue response's result script, compiled for its errors.

        Args:
            text: The result.
            speaker: The responding actor (its script's locals are the result's).
            compiler: :meth:`script_compiler`'s, when the caller has it.

        Returns:
            ``{"messages": [{"line", "level", "message"}], "speaker_script": bool}``.
        """
        compiler = compiler or self.script_compiler()
        messages = compiler.check_result(text, speaker)
        return {
            "messages": [{"line": m.line, "level": m.level, "message": m.text} for m in messages],
            "speaker_script": bool(speaker) and compiler.script_locals(speaker) is not None,
        }

    def set_script_text(self, found: Found, text: str) -> dict[str, Any]:
        """Save a script's source, and its bytecode with it when it compiles.

        The text is queued as any field is. When it compiles (and the Rust backend is
        built, which writes the record's fields in the form the patch stores), the
        header, locals and bytecode are queued beside it, so Morrowind.exe runs the new
        script too; OpenMW compiles the text itself either way.

        Args:
            found: The script.
            text: Its new source.

        Returns:
            ``{"compiled": bool, "messages": [...]}``: whether the bytecode was rebuilt,
            and the compiler's errors and warnings.

        Raises:
            EditorError: For a record that is not a script.
        """
        if found.record_type != "Script":
            raise EditorError(f"{found.key} is not a script")
        self.set_field(found, "text", text)
        compiler = self.script_compiler()
        result = compiler.compile(text)
        messages = [{"line": m.line, "level": m.level, "message": m.text} for m in result.messages]
        record_json = compiler.compile_record(found.key, text) if result.ok else None
        if record_json is None:
            return {"compiled": False, "messages": messages}
        built = json.loads(record_json)
        paths = (
            "header.num_shorts",
            "header.num_longs",
            "header.num_floats",
            "header.bytecode_length",
            "header.variables_length",
            "variables",
            "bytecode",
        )
        for path in paths:
            value, present = value_at(built, path)
            if present and value_at(found.record, path)[1]:
                self.set_field(found, path, value)
        return {"compiled": True, "messages": messages}

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
            "new": found.new,
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
        if found.new:
            # The patch's own record: the change is the record.
            made = self.queue.new_record(found.record_type, found.key)
            if made is None:
                raise EditorError(f"{found.key} is no longer in the patch")
            record = copy.deepcopy(dict(made.record))
            set_at(record, path, value)
            self.queue.add_new_record(replace(made, record=record))
            self.save_journal()
            return
        if value == current:
            self.queue.remove_field(found.record_type, found.key, path)
        else:
            self.queue.add_field(found.record_type, found.key, FieldValue(path=path, value=value))
            self.queue.set_base(found.record_type, found.key, found.winner)
        self.save_journal()

    def set_many(self, founds: Sequence[Found], path: str, raw: object) -> dict[str, Any]:
        """One field set to one value on many records: one change, one undo step.

        Args:
            founds: The records (:meth:`find`'s).
            path: The field's dotted path.
            raw: The value, as the viewer sent it (coerced per record, as
                :meth:`set_field` does).

        Returns:
            ``{"changed": n, "failed": [{"id", "error"}]}`` - a record without the field,
            or that refuses the value, is left as it was and named.
        """
        changed, failed = 0, []
        with self.batch():
            for found in founds:
                error = self._try_set(found, path, raw)
                if error is None:
                    changed += 1
                else:
                    failed.append({"id": found.key, "error": error})
        return {"changed": changed, "failed": failed}

    @contextlib.contextmanager
    def batch(self) -> Iterator[None]:
        """Changes made inside are one change: one journal save, one undo step."""
        outer = bool(self.__dict__.get("_batching"))
        self.__dict__["_batching"] = True
        try:
            yield
        finally:
            self.__dict__["_batching"] = outer
            if not outer:
                self.save_journal()

    def _try_set(self, found: Found, path: str, raw: object) -> str | None:
        """:meth:`set_field`, its refusal given back as text (None when it was set)."""
        try:
            self.set_field(found, path, raw)
        except EditorError as exc:
            return str(exc)
        return None

    def revert(self, found: Found, path: str | None = None) -> None:
        """Drop the queued change to one field, or to the whole record.

        Args:
            found: The record.
            path: The field, or None for every queued change to the record.
        """
        if found.new:
            if path is not None:
                raise EditorError(
                    "a record the patch makes has nothing to go back to: set the value, "
                    "or remove the record"
                )
            self.queue.remove_new_record(found.record_type, found.key)
        elif path is None:
            self.queue.remove_record(found.record_type, found.key)
        else:
            self.queue.remove_field(found.record_type, found.key, path)
        self.save_journal()

    def _new_id(self, tag: str, record_type: str, new_id: str) -> str:
        """An id for a record the patch makes, checked.

        Not empty, not too long, and not one the load order (or the patch) already has.

        Args:
            tag: The type's tag.
            record_type: The type.
            new_id: The id asked for.

        Returns:
            The id, trimmed.

        Raises:
            EditorError: When it cannot be used.
        """
        rid = new_id.strip()
        if not rid:
            raise EditorError("the record needs an id")
        if len(rid.encode("utf-8")) > MAX_ID:
            raise EditorError(f"an id is at most {MAX_ID} bytes")
        if self.find(tag, rid) is not None:
            raise EditorError(f"{rid} is already a {record_type} id in this load order")
        return rid

    def insert(self, tag: str, new_id: str) -> Found:
        """Make a blank record of a type under a new id, in the patch (and journal it).

        Every field at the type's default; a script a bare ``begin``/``end``.

        Args:
            tag: The type's tag (``STAT``).
            new_id: Its id.

        Returns:
            The record, as :meth:`find` finds it.

        Raises:
            EditorError: An unknown type, one made this way is not (as for
                :meth:`duplicate`, scripts aside), or an id that cannot be used.
        """
        from wraithguard.esp.json import _by_name, record_to_json

        tag = tag.upper().ljust(4, "_")
        record_type = TAG_TO_TYPE.get(tag)
        cls = _by_name().get(record_type or "")
        if record_type is None or cls is None:
            raise EditorError(f"no record type has the tag {tag}")
        if record_type in UNCOPYABLE - {"Script"}:
            raise EditorError(f"a {record_type} record cannot be made this way")
        rid = self._new_id(tag, record_type, new_id)
        record = record_to_json(cls())
        record["id"] = rid
        if record_type == "Script":
            record["text"] = f"begin {rid}\n\nend\n"
        self.queue.add_new_record(NewRecord(record_type, rid, record, ""))
        self.save_journal()
        return Found(tag, record_type, rid, ((PATCH, record),), new=True)

    def duplicate(self, found: Found, new_id: str) -> Found:
        """Make a copy of a record under a new id, in the patch (and save the journal).

        The Construction Set's way of making a record: change the id of one and save it
        as new. The copy is the patch's own; what it names (a script, a sound, items) is
        the original's, so the plugin it came from becomes a master.

        Args:
            found: The record to copy.
            new_id: The copy's id.

        Returns:
            The copy, as :meth:`find` finds it.

        Raises:
            EditorError: A type that cannot be copied this way, an empty or too long id,
                or one a plugin of this load order (or the patch) already uses.
        """
        if found.record_type in UNCOPYABLE - {"Script"}:
            raise EditorError(f"a {found.record_type} record cannot be copied under a new id")
        rid = self._new_id(found.tag, found.record_type, new_id)
        record = copy.deepcopy(found.record)
        record["id"] = rid
        if found.record_type == "Script":
            # The text names the script: its `begin` line takes the new id.
            record["text"] = re.sub(
                r"^(\s*begin\s+)(\"[^\"]*\"|\S+)",
                lambda m: m.group(1) + rid,
                str(record.get("text") or ""),
                count=1,
                flags=re.IGNORECASE | re.MULTILINE,
            )
        flags = str(record.get("flags") or "")
        if "DELETED" in flags:
            record["flags"] = " | ".join(f for f in flags.split(" | ") if f.strip() != "DELETED")
        source = "" if found.new else found.winner
        self.queue.add_new_record(NewRecord(found.record_type, rid, record, source))
        self.save_journal()
        return Found(found.tag, found.record_type, rid, ((PATCH, record),), new=True)

    def pending(self) -> list[dict[str, Any]]:
        """Everything the patch would carry, editor changes and conflict choices.

        Returns:
            One ``{"tag", "type", "id", "whole", "changes"}`` per record; each change
            ``{"path", "value"}`` (typed) or ``{"path", "plugin"}`` (taken from one).
            A changed placed object is ``type`` ``Reference``, with ``ref`` ``{cell,
            origin, refr, plugins}``; a new one ``NewReference``, with ``new`` ``{cell,
            uid, id, tag, plugins}`` and its fields as the changes. A record the patch
            makes has ``made`` ``{source, name, mesh}``.
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
        out.extend(
            {
                "tag": "",
                "type": "Reference",
                "id": f"{e.origin}:{e.refr_index} in {e.cell}",
                "ref": {
                    "cell": e.cell,
                    "origin": e.origin,
                    "refr": e.refr_index,
                    "plugins": list(e.plugins),
                },
                "whole": None,
                "changes": [{"path": k, "value": v} for k, v in e.changes.items()],
            }
            for e in self.queue.ref_edits
        )
        out.extend(
            {
                "tag": tag_of(m.record_type),
                "type": m.record_type,
                "id": m.key,
                "whole": None,
                "made": {
                    "source": m.source,
                    "name": m.record.get("name", ""),
                    "mesh": m.record.get("mesh", ""),
                },
                "changes": [],
            }
            for m in self.queue.new_records
        )
        out.extend(
            {
                "tag": "",
                "type": "NewReference",
                "id": f"{n.fields.get('id', '')} (new) in {n.cell}",
                "new": {
                    "cell": n.cell,
                    "uid": n.uid,
                    "id": n.fields.get("id", ""),
                    "tag": n.tag,
                    "plugins": list(n.plugins),
                },
                "whole": None,
                "changes": [{"path": k, "value": v} for k, v in n.fields.items()],
            }
            for n in self.queue.new_refs
        )
        return out

    # -- placed objects ---------------------------------------------------------------

    def _masters_of(self, plugin: str) -> list[str]:
        """A plugin's master list, from its header (read once)."""
        low = plugin.lower()
        with self._lock:
            hit = self._masters.get(low)
        if hit is not None:
            return hit
        name_path = self._paths.get(low)
        masters: list[str] = []
        if name_path is not None:
            try:
                masters = master_names(self._read(name_path[1], "TES3"))
            except (OSError, ValueError) as exc:
                LOG.warning("editor: cannot read %s: %s", name_path[1], exc)
        with self._lock:
            self._masters[low] = masters
        return masters

    def find_ref(
        self, cell: str, origin: str, refr_index: int, plugins: Sequence[str] | None = None
    ) -> FoundRef | None:
        """A placed object, from every plugin with a CELL record for its cell.

        Args:
            cell: The cell's key (an interior's name, or ``"(x, y)"``).
            origin: The plugin that created the reference (any case).
            refr_index: Its index there.
            plugins: The plugins with the cell, when the caller knows (the viewer
                does); else every plugin is looked in.

        Returns:
            The reference, or None when no version of the cell has it.
        """
        known = [p for p in (plugins or self.order) if p.lower() in self._rank]
        known.sort(key=lambda p: self._rank[p.lower()])
        want = cell.strip().lower()
        versions: list[tuple[str, Mapping[str, Any]]] = []
        for name in known:
            rec = self._records(name, "CELL").get(want)
            if rec is not None and rec.get("type") == "Cell":
                versions.append((self._paths[name.lower()][0], rec))
        masters_of = {p: self._masters_of(p) for p, _ in versions}
        won = winning_reference(versions, masters_of, origin, refr_index) if versions else None
        if won is None:
            # Moved here from another exterior cell (MVRF): its record is in the cell it
            # left, which is where it is changed.
            home = self._home_cell(origin, refr_index) if exterior_grid(cell) else None
            if home is None or home.lower() == want:
                return None
            return self.find_ref(home, origin, refr_index)
        spelled = self._paths.get(origin.lower(), (origin, None))[0]
        return FoundRef(
            record_key(versions[-1][1]),
            spelled,
            refr_index,
            tuple(p for p, _ in versions),
            won[0],
            won[1],
        )

    def _home_cell(self, origin: str, refr_index: int) -> str | None:
        """The exterior cell whose records hold a reference, or None.

        One moved out of a cell keeps its key there. Reads every plugin's cells (once).

        Args:
            origin: The plugin that created the reference.
            refr_index: Its index.

        Returns:
            The cell's key.
        """
        from wraithguard.patch.refedit import _origin

        for plugin in self.order:
            masters = self._masters_of(plugin)
            name = self._paths[plugin.lower()][0]
            for key, rec in self._records(plugin, "CELL").items():
                if exterior_grid(key) is None:
                    continue
                for ref in rec.get("references") or []:
                    if not isinstance(ref, dict) or ref.get("refr_index") != refr_index:
                        continue
                    at = _origin(name, masters, int(ref.get("mast_index", -1)))
                    if at is not None and at.lower() == origin.lower():
                        return record_key(rec)
        return None

    def _ref_queued(self, found: FoundRef) -> dict[str, Any]:
        """The changes queued for a placed object."""
        ident = (found.cell.lower(), found.origin.lower(), found.refr_index)
        edit = next((e for e in self.queue.ref_edits if e.ident == ident), None)
        return dict(edit.changes) if edit is not None else {}

    def ref_view(self, found: FoundRef) -> dict[str, Any]:
        """What the reference dialog shows.

        Args:
            found: The reference.

        Returns:
            ``{"cell", "origin", "refr", "id", "winner", "plugins", "fields"}``; each
            field ``{"path", "value", "kind", "editable"}``, plus ``queued`` when a
            change waits for it. ``value`` is None for a field the reference does not
            have.
        """
        queued = self._ref_queued(found)
        fields = []
        for name, kind in REF_KINDS.items():
            item: dict[str, Any] = {
                "path": name,
                "value": found.ref.get(name),
                "kind": kind,
                "editable": True,
            }
            if name in queued:
                item["queued"] = queued[name]
            fields.append(item)
        return {
            "cell": found.cell,
            "origin": found.origin,
            "refr": found.refr_index,
            "id": found.ref.get("id", ""),
            "winner": found.winner,
            "plugins": list(found.plugins),
            "fields": fields,
        }

    def set_ref_field(self, found: FoundRef, path: str, raw: object) -> None:
        """Queue a change to a placed object (and save the journal).

        A value equal to what the load order already has drops the change instead.

        Args:
            found: The reference.
            path: The field (:data:`REF_KINDS`).
            raw: The value, as the viewer sent it.

        Raises:
            EditorError: For a field not changed here, or a value it cannot hold.
        """
        value = coerce_ref(path, raw)
        self._queue_ref(found, path, value)
        home = exterior_grid(found.cell)
        if path == "translation" and home is not None and isinstance(value, list):
            # Moved out of its exterior cell: it stays in this cell's record, and says where
            # it is now (MVRF/CNDT, as merge_to_master moves one) - or no longer, once back.
            now = grid_of(value)
            self._queue_ref(found, "moved_cell", None if now == home else list(now))
        self.save_journal()

    def _queue_ref(self, found: FoundRef, path: str, value: object) -> None:
        """Queue one field's value, or drop the change when the load order has it already."""
        if same_value(value, found.ref.get(path)):
            self.queue.remove_ref_edit(found.cell, found.origin, found.refr_index, path)
        else:
            self.queue.add_ref_edit(
                RefEdit(found.cell, found.origin, found.refr_index, {path: value}, found.plugins)
            )

    def revert_ref(self, found: FoundRef, path: str | None = None) -> None:
        """Drop the change to one field of a placed object, or all of them.

        Args:
            found: The reference.
            path: The field, or None for every change to it.
        """
        self.queue.remove_ref_edit(found.cell, found.origin, found.refr_index, path)
        if path == "translation":  # the cell it moved to went with the move
            self.queue.remove_ref_edit(found.cell, found.origin, found.refr_index, "moved_cell")
        self.save_journal()

    # -- new references ---------------------------------------------------------------

    #: Tags placed persistent: the game expects actors to be (the tes3 crate's note).
    PERSISTENT_TAGS: Final = frozenset({"NPC_", "CREA"})

    # -- path grids ----------------------------------------------------------------------

    @staticmethod
    def pathgrid_key(cell: str) -> tuple[str, tuple[int, int] | None]:
        """A cell as its path grid's key, and its exterior grid (None inside).

        Args:
            cell: As the viewer names it: ``"x,y"``, ``"(x, y)"``, or an interior's name
                (``int:`` allowed).

        Returns:
            ``(key, grid)``.
        """
        name = cell.strip()
        if name.lower().startswith("int:"):
            return name[4:].strip(), None
        m = re.fullmatch(r"\(?\s*(-?\d+)\s*,\s*(-?\d+)\s*\)?", name)
        if m:
            grid = (int(m.group(1)), int(m.group(2)))
            return f"({grid[0]}, {grid[1]})", grid
        return name, None

    def _pathgrid_record(self, found: Found) -> dict[str, Any]:
        """The path grid as the patch would write it: the winner with queued values."""
        if found.new:
            return copy.deepcopy(dict(found.record))
        record = copy.deepcopy(dict(found.record))
        for c in self.queue.fields.get((found.record_type, found.key), []):
            if isinstance(c, FieldValue):
                set_at(record, c.path, c.value)
        return record

    def pathgrid_view(self, cell: str) -> dict[str, Any]:
        """A cell's path grid for the render window, as the patch would write it.

        Args:
            cell: The cell (:meth:`pathgrid_key`).

        Returns:
            ``{key, grid, plugins, queued, new, granularity, points:[[x, y, z]],
            edges:[[a, b]]}``. Points are as the record holds them - an exterior's
            relative to its cell's south-west corner. Each link is listed once per
            direction it is stored in; most go both ways.
        """
        from wraithguard.tes3fields.pathgrid import PathGridDecodeError, decode_connections

        key, grid = self.pathgrid_key(cell)
        found = self.find("PGRD", key)
        out: dict[str, Any] = {
            "key": key,
            "grid": list(grid) if grid else None,
            "points": [],
            "edges": [],
        }
        if found is None:
            out.update(plugins=[], queued=False, new=False, granularity=1024)
            return out
        rec = self._pathgrid_record(found)
        points = list(rec.get("points") or [])
        counts = [int(p.get("connection_count") or 0) for p in points]
        raw = rec.get("connections") or []
        if isinstance(raw, (bytes, bytearray, str)):
            try:
                targets = decode_connections(
                    bytes(raw) if isinstance(raw, bytearray) else raw, expected_edges=sum(counts)
                )
            except PathGridDecodeError:
                targets = []
        else:
            targets = [int(v) for v in raw]
        edges: list[list[int]] = []
        at = 0
        for i, n in enumerate(counts):
            edges.extend([i, t] for t in targets[at : at + n] if 0 <= t < len(points))
            at += n
        data = rec.get("data") or {}
        out.update(
            plugins=[p for p, _ in found.versions],
            queued=found.new or bool(self.queue.fields.get((found.record_type, found.key))),
            new=found.new,
            granularity=int(data.get("granularity") or 1024),
            points=[[int(v) for v in (p.get("location") or [0, 0, 0])] for p in points],
            edges=edges,
        )
        return out

    def set_pathgrid(
        self, cell: str, points: Sequence[Sequence[float]], edges: Sequence[Sequence[int]]
    ) -> dict[str, Any]:
        """Queue a cell's whole path grid: its points and links.

        A cell with no path grid gets one, the patch's own.

        Args:
            cell: The cell (:meth:`pathgrid_key`).
            points: ``[x, y, z]`` each, as the record holds them.
            edges: ``[a, b]`` each, one way; both directions for a two-way link.

        Returns:
            :meth:`pathgrid_view` after.

        Raises:
            EditorError: For a point that is not three numbers, a link to no point or to
                itself, or more points than a path grid holds.
        """
        key, grid = self.pathgrid_key(cell)
        if len(points) > 65535:
            raise EditorError("a path grid holds at most 65535 points")
        locs: list[list[int]] = []
        for i, p in enumerate(points):
            if not isinstance(p, (list, tuple)) or len(p) != 3:
                raise EditorError(f"point {i} is not [x, y, z]")
            try:
                locs.append([round(float(v)) for v in p])
            except (TypeError, ValueError):
                raise EditorError(f"point {i} is not [x, y, z]") from None
        links: dict[int, list[int]] = {i: [] for i in range(len(locs))}
        for e in edges:
            a, b = int(e[0]), int(e[1])
            if not (0 <= a < len(locs) and 0 <= b < len(locs)) or a == b:
                raise EditorError(f"the link {a} -> {b} does not join two points")
            if b not in links[a]:
                links[a].append(b)
        found = self.find("PGRD", key)
        old = list(self._pathgrid_record(found).get("points") or []) if found else []
        auto = int(old[0].get("auto_generated") or 0) if old and isinstance(old[0], dict) else 1
        new_points = [
            {
                "location": loc,
                "auto_generated": (
                    int(old[i].get("auto_generated") or 0)
                    if i < len(old) and isinstance(old[i], dict)
                    else auto
                ),
                "connection_count": len(links[i]),
            }
            for i, loc in enumerate(locs)
        ]
        conns = [t for i in range(len(locs)) for t in sorted(links[i])]
        if found is None:
            cell_found = self.find("CELL", key)
            record: dict[str, Any] = {
                "type": "PathGrid",
                "flags": "",
                "cell": "" if grid else key,
                "data": {
                    "grid": list(grid) if grid else [0, 0],
                    "granularity": 1024,
                    "point_count": len(locs),
                },
                "points": new_points,
                "connections": conns,
            }
            if cell_found is not None and grid is not None:
                record["cell"] = str(cell_found.record.get("name") or "")
            source = cell_found.winner if cell_found is not None and not cell_found.new else ""
            self.queue.add_new_record(NewRecord("PathGrid", key, record, source))
            self.save_journal()
            return self.pathgrid_view(cell)
        if found.new:
            made = self.queue.new_record(found.record_type, found.key)
            if made is None:
                raise EditorError(f"{found.key} is no longer in the patch")
            record = copy.deepcopy(dict(made.record))
            record["points"], record["connections"] = new_points, conns
            record.setdefault("data", {})["point_count"] = len(locs)
            self.queue.add_new_record(replace(made, record=record))
            self.save_journal()
            return self.pathgrid_view(cell)
        for path, value in (
            ("points", new_points),
            ("connections", conns),
            ("data.point_count", len(locs)),
        ):
            self.queue.add_field(found.record_type, found.key, FieldValue(path=path, value=value))
        self.queue.set_base(found.record_type, found.key, found.winner)
        self.save_journal()
        return self.pathgrid_view(cell)

    def revert_pathgrid(self, cell: str) -> dict[str, Any]:
        """Drop a cell's queued path grid (a new one with it).

        Args:
            cell: The cell (:meth:`pathgrid_key`).

        Returns:
            :meth:`pathgrid_view` after.
        """
        key, _ = self.pathgrid_key(cell)
        found = self.find("PGRD", key)
        if found is not None:
            if found.new:
                self.queue.remove_new_record(found.record_type, found.key)
                self.save_journal()
            else:
                self.revert(found)
        return self.pathgrid_view(cell)

    def _cell_plugins(self, cell: str, plugins: Sequence[str] | None) -> tuple[str, ...]:
        """The plugins with a CELL record for a cell, in load order.

        The viewer's list, checked, or every plugin looked in.
        """
        known = [p for p in (plugins or self.order) if p.lower() in self._rank]
        known.sort(key=lambda p: self._rank[p.lower()])
        want = cell.strip().lower()
        return tuple(self._paths[p.lower()][0] for p in known if want in self._records(p, "CELL"))

    def place(
        self,
        cell: str,
        found: Found,
        translation: object,
        rotation: object = None,
        plugins: Sequence[str] | None = None,
    ) -> NewRef:
        """Queue a new reference to a record (and save the journal).

        Args:
            cell: The cell's key (an interior's name, an exterior's ``"(x, y)"``).
            found: What it places (:meth:`find`).
            translation: Where, as the viewer sent it.
            rotation: Its rotation (radians), or None for none.
            plugins: The plugins with the cell, when the viewer knows.

        Returns:
            The reference queued.

        Raises:
            EditorError: A cell no plugin has, or a position or rotation that is not
                three numbers.
        """
        import secrets

        holders = self._cell_plugins(cell, plugins)
        base = "" if found.new else found.winner
        own = self.queue.new_record("Cell", self._cell_key(cell))
        if holders:
            rec = self._records(holders[-1], "CELL")[cell.strip().lower()]
        elif own is not None:  # a cell the patch makes (a new one, or a copy)
            rec = dict(own.record)
        else:
            raise EditorError(f"no plugin of this load order has the cell {cell}")
        fields: dict[str, Any] = {
            "id": found.key,
            "translation": coerce_ref("translation", translation),
            "rotation": coerce_ref("rotation", rotation if rotation is not None else [0, 0, 0]),
            "temporary": found.tag not in self.PERSISTENT_TAGS,
        }
        new = NewRef(
            record_key(rec), f"new-{secrets.token_hex(4)}", fields, base, holders, found.tag
        )
        self.queue.add_new_ref(new)
        self.save_journal()
        return new

    #: The longest cell name the game reads (64 bytes with the terminator).
    MAX_CELL_NAME: Final = 63

    @staticmethod
    def _cell_key(cell: str) -> str:
        """A cell as the pool keys it: an interior's name (``int:`` dropped), ``(x, y)``."""
        name = cell.strip()
        return name[4:].strip() if name.lower().startswith("int:") else name

    def new_cell(self, cell: str) -> dict[str, Any]:
        """A cell of the patch's own, from nothing (and the journal saved).

        A blank interior under a name (with water off, and its lighting a dim grey the
        reference dialog's cell can change), or an exterior square ``(x, y)`` the load
        order does not have (no land: the game draws it flat, at the sea).

        Args:
            cell: An interior's name, or ``(x, y)``.

        Returns:
            ``{"cell": its key, "interior": bool}``.

        Raises:
            EditorError: For a name that is empty, too long or taken, or a square some
                plugin has.
        """
        from wraithguard.esp.flags import CellFlags
        from wraithguard.esp.json import record_to_json
        from wraithguard.esp.records.cell import AtmosphereData, Cell, CellData

        key = self._cell_key(cell)
        grid = exterior_grid(key)
        if not key:
            raise EditorError("a new cell needs a name, or (x, y) for an exterior square")
        if self.find("CELL", key) is not None:
            raise EditorError(f"the load order has the cell {key} already")
        if grid is not None:
            record = record_to_json(Cell(name="", data=CellData(grid=grid)))
        else:
            if len(key.encode("cp1252", "replace")) > self.MAX_CELL_NAME:
                raise EditorError(f"a cell name is at most {self.MAX_CELL_NAME} characters")
            grey = b"\x40\x40\x40\x00"
            record = record_to_json(
                Cell(
                    name=key,
                    data=CellData(cell_flags=CellFlags.IS_INTERIOR, grid=(0, 0)),
                    atmosphere_data=AtmosphereData(
                        ambient_color=grey,
                        sunlight_color=grey,
                        fog_color=b"\x00\x00\x00\x00",
                        fog_density=1.0,
                    ),
                )
            )
        self.queue.add_new_record(NewRecord("Cell", key, record, ""))
        self.save_journal()
        return {"cell": key, "interior": grid is None}

    def place_many(
        self, cell: str, items: Sequence[tuple[Found, object, object, float]]
    ) -> list[NewRef]:
        """Several new references at once (a prefab), as one change.

        Args:
            cell: The cell they go in; for an exterior, each goes in the square its
                position is in.
            items: ``(record, translation, rotation, scale)`` each.

        Returns:
            The references queued.
        """
        placed: list[NewRef] = []
        with self.batch():
            for found, translation, rotation, scale in items:
                into = cell
                if exterior_grid(self._cell_key(cell)) is not None:
                    gx, gy = grid_of(coerce_ref("translation", translation))  # type: ignore[arg-type]
                    into = f"({gx}, {gy})"
                new = self.place(into, found, translation, rotation)
                if scale and abs(float(scale) - 1.0) > 1e-6:
                    new = self.set_new_field(new, "scale", float(scale))
                placed.append(new)
        return placed

    def duplicate_cell(self, cell: str, new_name: str) -> dict[str, Any]:
        """Make a copy of a whole interior: a new cell of the patch's own.

        Its references are placed anew and its path grid comes with it.

        The copy is the cell as the load order resolves it - every plugin's references
        merged, deleted ones left out, and the changes queued in the patch pool applied
        (references moved, the patch's own new ones included). Each reference becomes
        one the patch adds (``(0, n)``, numbered when the patch is written), so the copy
        names nothing of the original's.

        Args:
            cell: The interior's name (any case; ``int:`` allowed).
            new_name: The copy's name.

        Returns:
            ``{cell, refs, pathgrid}``: the copy's name, how many references it got, and
            whether a path grid came with it.

        Raises:
            EditorError: For an exterior (a grid square cannot be copied to another), a
                cell no plugin has, or a name that is empty, too long or taken.
        """
        import secrets

        src = cell.strip()
        if src.lower().startswith("int:"):
            src = src[4:].strip()
        if exterior_grid(src) is not None or re.fullmatch(r"-?\d+\s*,\s*-?\d+", src):
            raise EditorError("only an interior can be copied: an exterior is a grid square")
        name = new_name.strip()
        if not name:
            raise EditorError("the copy needs a name")
        if len(name.encode("cp1252", "replace")) > self.MAX_CELL_NAME:
            raise EditorError(f"a cell name is at most {self.MAX_CELL_NAME} characters")
        if self.find("CELL", name) is not None:
            raise EditorError(f"there is a cell named {name} already")
        holders = self._cell_plugins(src, None)
        if not holders:
            raise EditorError(f"no plugin of this load order has the cell {src}")
        versions = [(p, self._records(p, "CELL")[src.lower()]) for p in holders]
        masters_of = {p: self._masters_of(p) for p in holders}
        winner = copy.deepcopy(dict(versions[-1][1]))
        winner.pop("references", None)
        winner["name"] = name
        key = record_key(versions[-1][1])
        # Every reference as the load order resolves it, the pool's changes on top.
        edits = {
            (e.origin.lower(), e.refr_index): e
            for e in self.queue.ref_edits
            if e.cell.lower() == key.lower()
        }
        seen: dict[tuple[str, int], str] = {}
        for plugin, rec in versions:
            for ref in rec.get("references") or []:
                if not isinstance(ref, dict):
                    continue
                mast = int(ref.get("mast_index", -1))
                masters = masters_of.get(plugin, ())
                origin = (
                    plugin
                    if mast == 0
                    else (masters[mast - 1] if 0 < mast <= len(masters) else None)
                )
                if origin is not None:
                    seen[(origin.lower(), int(ref.get("refr_index", 0)))] = origin
        placed: list[NewRef] = []
        for (low, refr), origin in seen.items():
            won = winning_reference(versions, masters_of, origin, refr)
            if won is None:
                continue
            plugin, ref = won
            edit = edits.get((low, refr))
            if edit is not None:
                ref = {**ref, **dict(edit.changes)}
            if ref.get("deleted") or not ref.get("id") or ref.get("translation") is None:
                continue
            fields = {
                k: copy.deepcopy(v)
                for k, v in ref.items()
                if (k in REF_FIELDS or k == "id")
                and k not in ("deleted", "moved_cell")
                and v is not None
            }
            placed.append(NewRef(name, f"new-{secrets.token_hex(4)}", fields, plugin, (), ""))
        placed.extend(
            NewRef(name, f"new-{secrets.token_hex(4)}", dict(n.fields), n.base_plugin, (), n.tag)
            for n in self.queue.new_refs
            if n.cell.lower() == key.lower()
        )
        self.queue.add_new_record(NewRecord("Cell", name, winner, versions[-1][0]))
        for new in placed:
            self.queue.add_new_ref(new)
        # The path grid (as the patch would write it) comes too, under the new name.
        grid = self.find("PGRD", src)
        has_grid = grid is not None
        if grid is not None:
            rec = self._pathgrid_record(grid)
            rec["cell"] = name
            data = rec.get("data")
            if isinstance(data, dict):
                data["grid"] = [0, 0]
            self.queue.add_new_record(
                NewRecord("PathGrid", name, rec, "" if grid.new else grid.winner)
            )
        self.save_journal()
        return {"cell": name, "refs": len(placed), "pathgrid": has_grid}

    def find_new(self, cell: str, uid: str) -> NewRef | None:
        """A queued new reference, by its cell and editor name.

        Args:
            cell: The cell's key.
            uid: Its editor name.

        Returns:
            It, or None.
        """
        del cell  # the uid is the patch's own and unique; the cell may have moved under it
        return next((n for n in self.queue.new_refs if n.uid == uid), None)

    def new_view(self, new: NewRef) -> dict[str, Any]:
        """The reference dialog for a new reference: its fields, each as it will be written.

        Args:
            new: The reference.

        Returns:
            ``{"cell", "uid", "id", "tag", "new", "plugins", "fields"}``, fields as
            :meth:`ref_view` has them (no ``queued``: all of it is the patch's).
        """
        return {
            "cell": new.cell,
            "uid": new.uid,
            "id": new.fields.get("id", ""),
            "tag": new.tag,
            "new": True,
            "plugins": list(new.plugins),
            "fields": [
                {"path": name, "value": new.fields.get(name), "kind": kind, "editable": True}
                for name, kind in REF_KINDS.items()
            ],
        }

    def set_new_field(self, new: NewRef, path: str, raw: object) -> NewRef:
        """Change a field of a new reference (and save the journal).

        Args:
            new: The reference.
            path: The field (:data:`REF_KINDS`).
            raw: The value, as the viewer sent it.

        Returns:
            The reference as it now is.

        Raises:
            EditorError: For a field not changed here, or a value it cannot hold (a
                position is always three numbers).
        """
        from dataclasses import replace

        value = coerce_ref(path, raw)
        home = exterior_grid(new.cell)
        if path == "translation" and home is not None and isinstance(value, list):
            now = grid_of(value)
            if now != home:
                # A new reference belongs to the cell it stands in: it moves to that one.
                cell = f"({now[0]}, {now[1]})"
                holders = self._cell_plugins(cell, None)
                if not holders:
                    raise EditorError(f"no plugin of this load order has the cell {cell}")
                self.queue.remove_new_ref(new.cell, new.uid)
                new = replace(new, cell=cell, plugins=holders)
                self.queue.add_new_ref(new)
        self.queue.add_new_ref(NewRef(new.cell, new.uid, {path: value}))
        self.save_journal()
        return self.find_new(new.cell, new.uid) or new

    def remove_new(self, new: NewRef) -> None:
        """Drop a new reference from the patch (and save the journal).

        Args:
            new: The reference.
        """
        self.queue.remove_new_ref(new.cell, new.uid)
        self.save_journal()

    # -- the journal ------------------------------------------------------------------

    #: The most undo steps kept.
    UNDO_LIMIT: Final = 200

    def save_journal(self) -> None:
        """Write the whole queue to the journal, and keep the pool before as an undo step.

        See :func:`save_queue` for the journal.

        Every change the editor makes ends here, so this is the one place the history
        is kept: one Ctrl+Z for a field, a moved object, a path grid, a placed or deleted
        reference, a moved response, a copied cell alike.
        """
        if self.__dict__.get("_batching"):
            return  # the batch saves (and is one undo step) when it ends
        self._record_step()
        if self.journal is not None:
            save_queue(self.queue, self.journal)

    def _hist(self) -> dict[str, Any]:
        """The undo history: ``undo``/``redo`` pool snapshots, and the pool ``last`` seen."""
        return self._history

    def _record_step(self) -> None:
        """The pool before this change onto the undo list, when it changed."""
        h = self._hist()
        now = self.queue.snapshot()
        if now == h["last"]:
            return
        h["undo"].append(h["last"])
        del h["undo"][: -self.UNDO_LIMIT]
        h["redo"].clear()
        h["last"] = now

    def can_undo(self) -> dict[str, int]:
        """How many steps there are to undo and to redo."""
        h = self._hist()
        return {"undo": len(h["undo"]), "redo": len(h["redo"])}

    def undo(self) -> dict[str, Any]:
        """Put the pool back as it was before the last change (and save the journal).

        A change made to the pool outside the editor since (the Patch Builder) is part
        of the step it is undone with: the history is of the pool, not of one window.

        Returns:
            ``{"done": bool, "what": [the records or references it touched],
            "undo": n, "redo": n}``.
        """
        return self._step("undo", "redo")

    def redo(self) -> dict[str, Any]:
        """Make again what :meth:`undo` took back.

        Returns:
            As :meth:`undo`.
        """
        return self._step("redo", "undo")

    def _step(self, take: str, give: str) -> dict[str, Any]:
        """Move one step from the ``take`` list to the ``give`` list."""
        h = self._hist()
        if not h[take]:
            return {"done": False, "what": [], **self.can_undo()}
        before = self.pending()
        h[give].append(self.queue.snapshot())
        snap = h[take].pop()
        self.queue.restore_snapshot(snap)
        h["last"] = self.queue.snapshot()
        if self.journal is not None:
            save_queue(self.queue, self.journal)
        after = self.pending()
        return {"done": True, "what": _pending_diff(before, after), **self.can_undo()}

    def forget(self) -> None:
        """Remove the journal (the queue was written, or emptied)."""
        if self.journal is not None:
            forget_queue(self.journal)

    def restore(self) -> int:
        """Put the journal's decisions back into the queue (see :func:`restore_queue`).

        Returns:
            How many records it brought back.
        """
        n = restore_queue(self.queue, self.journal) if self.journal is not None else 0
        # What the journal brought back is where the history starts, not a step.
        self._history = {"undo": [], "redo": [], "last": self.queue.snapshot()}
        return n


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
        "made": [
            {
                "type": m.record_type,
                "key": m.key,
                "record": dict(m.record),
                "source": m.source,
                "topic": m.topic,
            }
            for m in queue.new_records
        ],
        "new": [
            {
                "cell": n.cell,
                "uid": n.uid,
                "fields": dict(n.fields),
                "base": n.base_plugin,
                "plugins": list(n.plugins),
                "tag": n.tag,
            }
            for n in queue.new_refs
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
        for m in doc.get("made") or []:
            if queue.new_record(m["type"], m["key"]) is None:
                queue.add_new_record(
                    NewRecord(
                        m["type"],
                        m["key"],
                        dict(m["record"]),
                        str(m.get("source") or ""),
                        str(m.get("topic") or ""),
                    )
                )
                n += 1
        placed = {p.ident for p in queue.new_refs}
        for r in doc.get("new") or []:
            new = NewRef(
                r["cell"],
                str(r["uid"]),
                dict(r["fields"]),
                str(r.get("base") or ""),
                tuple(r.get("plugins") or ()),
                str(r.get("tag") or ""),
            )
            if new.ident not in placed:
                queue.add_new_ref(new)
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
    "PATCH",
    "READ_ONLY",
    "REF_KINDS",
    "TAG_TO_TYPE",
    "UNCOPYABLE",
    "EditorError",
    "EditorSession",
    "Found",
    "FoundRef",
    "coerce",
    "coerce_ref",
    "exterior_grid",
    "forget_queue",
    "grid_of",
    "plugins_from_cfg",
    "restore_queue",
    "same_value",
    "save_queue",
    "tag_of",
]
