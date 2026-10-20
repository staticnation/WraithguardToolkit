"""A cache of plugin records on disk, so the editor reads a load order once.

Reading one tag (the cells, say) out of hundreds of plugins is minutes of file reading the
first time and nothing the second. Each entry is one plugin's records of one tag, valid
while that plugin's size and modification time are the ones it was read at; a plugin that
changed is simply read again. The cache is only ever a shortcut: any failure of it (a
locked, corrupt or missing database) is a miss, never an error in the editor.

Beside the records is a key index: which record keys (a cell's name or ``(x, y)``, an id)
each plugin has of a tag, with the same size-and-time stamp. "Is this cell name free?" and
"which plugins have this cell?" are then one indexed query over the whole load order rather
than reading every plugin's records. The index is filled whenever records are cached and
can be filled alone (:meth:`put_keys`), which keeps no record bodies: land records are
large.

The database is Rust's since port step 4.1, slice 2 (``viewer-shell/editcore/src/cache.rs``,
through ``wraithguard_native.RecordCacheDb``), the one the viewer's Editor will use. What it
stores is tagged JSON, zstd-compressed (``wraithguard_native.cache_dumps``): tuples stay
tuples and bytes stay bytes, and reading it back runs no code - it used to be pickles.

Copyright (c) 2026 StaticNation, GPL-2.0.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

#: How much of the file the record bodies may take before the largest tag's go (they are
#: read again when needed; the key index, which is small, stays). Land and dialogue are the
#: large ones.
MAX_BYTES: Final = 1 << 30

#: Reports kept for the load order as it is (the oldest go first): ``editcore::cache``'s.
_REPORTS_KEPT: Final = 2000


def parser_signature() -> str:
    """Which build of the native reader made what is cached.

    A new build can read records differently, and the plugins' own sizes and times would
    not show it, so the cache is stamped with the reader: its version and the size and
    time of its file (a rebuild changes those even when the version number does not).

    Returns:
        A short text, or ``""`` when the native reader is not there.
    """
    try:
        import wraithguard_native
    except ImportError:
        return ""
    where = getattr(wraithguard_native, "__file__", None)
    mark = ""
    if where:
        try:
            st = Path(where).stat()
        except OSError:
            mark = ""
        else:
            mark = f"{st.st_size}:{st.st_mtime_ns}"
    return f"{getattr(wraithguard_native, '__version__', '')}:{mark}"


def _dumps(value: object) -> bytes | None:
    """What the cache stores for a value, or None for a value it does not keep."""
    import wraithguard_native

    try:
        return bytes(wraithguard_native.cache_dumps(value))
    except (TypeError, ValueError):
        return None


def _loads(data: bytes | None) -> list[Any] | None:
    """The list a stored body holds, or None (nothing stored, or not a list)."""
    if data is None:
        return None
    import wraithguard_native

    try:
        out = wraithguard_native.cache_loads(data)
    except ValueError:
        return None
    return out if isinstance(out, list) else None


def _fs(paths: Iterable[Path]) -> list[str]:
    """The paths as text, for the native side."""
    return [os.fspath(p) for p in paths]


class RecordCache:
    """Plugin records of one tag, kept in a SQLite file (``editcore::cache``).

    One connection is shared by every thread (the editor reads from several), under a lock.
    """

    def __init__(self, path: Path, parser: str = "", max_bytes: int = MAX_BYTES) -> None:
        """Remember where the database is; it is opened on first use.

        Args:
            path: The database file (created when needed).
            parser: Which build of the native reader the records are made by
                (:func:`parser_signature`); a database made by another is emptied.
            max_bytes: The size the database may reach before record bodies are dropped.
        """
        import wraithguard_native

        self.path = path
        self.parser = parser
        self.max_bytes = max_bytes
        self._db = wraithguard_native.RecordCacheDb(os.fspath(path), parser, max_bytes)

    def get(self, plugin: Path, tag: str) -> list[Any] | None:
        """One plugin's cached records of a tag, when they are still current.

        Returns:
            The records, or None (never cached, or the plugin has changed since).
        """
        return _loads(self._db.get(os.fspath(plugin), tag))

    def put(
        self, plugin: Path, tag: str, records: list[Any], keys: Iterable[str] | None = None
    ) -> None:
        """Keep a plugin's records of a tag, as the plugin is now.

        Args:
            plugin: The plugin file.
            tag: The four-letter tag.
            records: What was read.
            keys: The records' keys (lower case), to index too; None leaves the index as it is.
        """
        blob = _dumps(records)
        if blob is None:
            return
        self._db.put(os.fspath(plugin), tag, blob, None if keys is None else list(keys))

    def size_bytes(self) -> int:
        """The bytes of the database in use (0 when the cache is out)."""
        return int(self._db.size_bytes())

    def put_keys(self, plugin: Path, tag: str, keys: Iterable[str]) -> None:
        """Index a plugin's keys of a tag (lower case) without keeping its records."""
        self._db.put_keys(os.fspath(plugin), tag, list(keys))

    def put_uses(self, plugin: Path, rows: Iterable[tuple[str, str, str, str, int]]) -> None:
        """Index every string a plugin's records hold (the use-report index).

        Args:
            plugin: The plugin file.
            rows: ``(value, record type, record key, paths as JSON, count)`` (usesindex).
        """
        self._db.put_uses(os.fspath(plugin), list(rows))

    def missing_uses(self, plugins: Sequence[Path]) -> list[Path]:
        """The plugins whose strings are not indexed as they now are (all if the cache is out)."""
        return [plugins[i] for i in self._db.missing_uses(_fs(plugins))]

    def uses_for(
        self, plugins: Sequence[Path], value: str
    ) -> list[tuple[Path, str, str, list[str], int]] | None:
        """Every record that holds a string, from the use-report index.

        Args:
            plugins: The load order's plugin files.
            value: The string, lower case.

        Returns:
            ``(plugin, record type, record key, paths, count)`` rows in load order and,
            within a plugin, file order; None when the index cannot answer (some plugin is
            not indexed as it now is, or the cache is out). Definition rows (count 0) are
            left out.
        """
        hits = self._db.uses_for(_fs(plugins), value)
        if hits is None:
            return None
        return [(plugins[i], rtype, key, list(paths), n) for i, rtype, key, paths, n in hits]

    def definers(
        self, plugins: Sequence[Path], records: Iterable[tuple[str, str]]
    ) -> dict[tuple[str, str], list[Path]] | None:
        """Which plugins define each record, in load order, from the use-report index.

        Args:
            plugins: The load order's plugin files.
            records: ``(record type, key in lower case)`` pairs.

        Returns:
            Each pair's defining plugins (load order), or None when the index cannot answer.
        """
        found = self._db.definers(_fs(plugins), list(records))
        if found is None:
            return None
        return {(pair[0], pair[1]): [plugins[i] for i in at] for pair, at in found.items()}

    def keys_current(self, plugin: Path, tag: str) -> bool:
        """Whether a plugin's keys of a tag are indexed and the plugin is unchanged since."""
        return bool(self._db.keys_current(os.fspath(plugin), tag))

    def missing_keys(self, plugins: Sequence[Path], tag: str) -> list[Path]:
        """The plugins whose keys of a tag still have to be indexed (all, if the cache is out)."""
        return [plugins[i] for i in self._db.missing_keys(_fs(plugins), tag)]

    def holders(self, plugins: Sequence[Path], tag: str, key: str) -> list[Path] | None:
        """Which of these plugins have a key, from the index.

        Returns:
            The plugins that have it (in the order given), or None when the index cannot
            answer: some plugin is not indexed as it now is, or the cache is out.
        """
        at = self._db.holders(_fs(plugins), tag, key)
        return None if at is None else [plugins[i] for i in at]

    def all_keys(self, plugins: Sequence[Path], tag: str) -> set[str] | None:
        """Every key of a tag over these plugins (lower case), from the index, or None."""
        keys = self._db.all_keys(_fs(plugins), tag)
        return None if keys is None else set(keys)

    @staticmethod
    def load_order_signature(plugins: Sequence[Path]) -> str | None:
        """One text for a load order: which plugins, in what order, as they now are.

        Returns:
            A hash of every plugin's path, size and time, or None when one cannot be
            looked at (nothing is then cached under it).
        """
        import wraithguard_native

        return wraithguard_native.RecordCacheDb.load_order_signature(_fs(plugins))

    def get_report(self, sig: str, kind: str, key: str) -> list[Any] | None:
        """A report kept for a load order (:meth:`load_order_signature`), or None."""
        return _loads(self._db.get_report(sig, kind, key))

    def put_report(self, sig: str, kind: str, key: str, value: list[Any]) -> None:
        """Keep a report for a load order; the oldest are dropped past a limit."""
        blob = _dumps(value)
        if blob is not None:
            self._db.put_report(sig, kind, key, blob)

    def clear(self) -> None:
        """Forget everything cached."""
        self._db.clear()

    def stats(self) -> dict[str, int]:
        """What the cache holds (empty when the cache is out).

        Returns:
            ``pages``, ``free`` (pages), ``page_size``, ``used`` (bytes), and a
            ``rows_<table>`` for each table.
        """
        return dict(self._db.stats())

    def compact(self) -> bool:
        """Give the disk back when more than half the file is free pages; whether it did."""
        return bool(self._db.compact())

    def close(self) -> None:
        """Close the database (it reopens on next use), compacting it first if mostly free."""
        self._db.close()

    def __enter__(self) -> RecordCache:
        """Use the cache in a ``with``: it is closed on the way out."""
        return self

    def __exit__(self, *_exc: object) -> None:
        """Close the database."""
        self.close()


__all__ = ["MAX_BYTES", "RecordCache", "parser_signature"]
