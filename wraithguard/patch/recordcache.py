"""A cache of plugin records on disk, so the editor reads a load order once.

Reading one tag (the cells, say) out of hundreds of plugins is minutes of file reading the
first time and nothing the second. Each entry is one plugin's records of one tag, valid
while that plugin's size and modification time are the ones it was read at; a plugin that
changed is simply read again. The cache is only ever a shortcut: any failure of it (a
locked, corrupt or missing database) is a miss, never an error in the editor.

Beside the records is a key index: which record keys (a cell's name or ``(x, y)``, an id)
each plugin has of a tag, with the same size-and-time stamp. "Is this cell name free?" and
"which plugins have this cell?" are then one indexed query over the whole load order rather
than reading (and unpickling) every plugin's records. The index is filled whenever records
are cached and can be filled alone (:meth:`put_keys`), which keeps no record bodies: land
records are large.

Copyright (c) 2026 StaticNation, GPL-2.0.
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import logging
import os
import pickle
import sqlite3
import threading
import zlib
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

LOG = logging.getLogger(__name__)

#: Bumped when what is stored changes shape; an older database is emptied, not read.
SCHEMA: Final = 3

#: The tables that hold what the native reader made of the plugins (emptied together).
_DERIVED: Final = (
    "records",
    "keys",
    "key_stamps",
    "reports",
    "uses",
    "use_records",
    "use_plugins",
)

#: Reports kept for the load order as it is (the oldest go first). One report per record
#: looked at, whatever the number of plugins; a report for any other load order is useless
#: and goes as soon as a new one is kept.
_REPORTS_KEPT: Final = 2000

#: How much of the file the record bodies may take before the largest tag's go (they are
#: read again when needed; the key index, which is small, stays). Land and dialogue are the
#: large ones.
MAX_BYTES: Final = 1 << 30

#: The pseudo-tag the use-report index is stamped under (in ``key_stamps``).
_USES_TAG: Final = "~USE"

#: Plugins asked about in one SQL statement (SQLite's limit on parameters is far above).
_CHUNK: Final = 400


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


def _stamp(plugin: Path) -> tuple[str, int, int] | None:
    """A plugin's cache key and the size and time it is valid for.

    Args:
        plugin: The plugin file.

    Returns:
        ``(key, size, mtime_ns)``, or None when the file cannot be looked at.
    """
    try:
        st = plugin.stat()
    except OSError:
        return None
    return os.path.normcase(str(plugin)), st.st_size, st.st_mtime_ns


class RecordCache:
    """Plugin records of one tag, kept in a SQLite file.

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
        self.max_bytes = max_bytes
        self.path = path
        self.parser = parser
        self._lock = threading.Lock()
        self._conn: sqlite3.Connection | None = None
        self._dead = False

    def _db(self) -> sqlite3.Connection | None:
        """The open connection, made on first use; None when the cache cannot be had."""
        if self._dead:
            return None
        if self._conn is not None:
            return self._conn
        conn: sqlite3.Connection | None = None
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            conn = sqlite3.connect(self.path, timeout=5.0, check_same_thread=False)
            conn.execute("PRAGMA journal_mode=WAL")
            conn.execute("PRAGMA synchronous=NORMAL")
            conn.execute("CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v INTEGER)")
            row = conn.execute("SELECT v FROM meta WHERE k='schema'").fetchone()
            made = conn.execute("SELECT v FROM meta WHERE k='parser'").fetchone()
            if row is None or row[0] != SCHEMA or made is None or made[0] != f"p:{self.parser}":
                # Another shape, or made by another build of the reader: none of it is
                # to be trusted, the key index with the records.
                for table in _DERIVED:
                    conn.execute(f"DROP TABLE IF EXISTS {table}")
                conn.execute("INSERT OR REPLACE INTO meta VALUES ('schema', ?)", (SCHEMA,))
                # Prefixed: the column is INTEGER-typed, which would turn digits-only text into a number.
                conn.execute(
                    "INSERT OR REPLACE INTO meta VALUES ('parser', ?)", (f"p:{self.parser}",)
                )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS records (plugin TEXT NOT NULL, tag TEXT NOT NULL,"
                " size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, data BLOB NOT NULL,"
                " PRIMARY KEY (plugin, tag))"
            )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS keys (tag TEXT NOT NULL, key TEXT NOT NULL,"
                " plugin TEXT NOT NULL, PRIMARY KEY (tag, key, plugin)) WITHOUT ROWID"
            )
            conn.execute("CREATE INDEX IF NOT EXISTS keys_plugin ON keys (plugin, tag)")
            conn.execute(
                "CREATE TABLE IF NOT EXISTS key_stamps (plugin TEXT NOT NULL, tag TEXT NOT NULL,"
                " size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, PRIMARY KEY (plugin, tag))"
            )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS use_plugins (pid INTEGER PRIMARY KEY,"
                " plugin TEXT NOT NULL UNIQUE)"
            )
            # One row per record (which plugin, what type, its key: kept once, not once per
            # string it holds) and one per (string, record) - a clustered table, no index of its own.
            conn.execute(
                "CREATE TABLE IF NOT EXISTS use_records (rid INTEGER PRIMARY KEY,"
                " pid INTEGER NOT NULL, rtype TEXT NOT NULL, key TEXT NOT NULL)"
            )
            conn.execute("CREATE INDEX IF NOT EXISTS use_records_pid ON use_records (pid)")
            conn.execute("CREATE INDEX IF NOT EXISTS use_records_key ON use_records (lower(key))")
            conn.execute(
                "CREATE TABLE IF NOT EXISTS uses (value TEXT NOT NULL, rid INTEGER NOT NULL,"
                " paths TEXT NOT NULL, n INTEGER NOT NULL, PRIMARY KEY (value, rid)) WITHOUT ROWID"
            )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS reports (sig TEXT NOT NULL, kind TEXT NOT NULL,"
                " key TEXT NOT NULL, data BLOB NOT NULL, PRIMARY KEY (sig, kind, key))"
            )
            self._forget_missing(conn)
            conn.commit()
        except (sqlite3.Error, OSError) as exc:
            LOG.warning("editor: record cache unavailable (%s): %s", self.path, exc)
            self._dead = True
            if conn is not None:
                # Opened, then refused (a file that is not a database): do not leave it open.
                with contextlib.suppress(sqlite3.Error):
                    conn.close()
            return None
        self._conn = conn
        return conn

    @staticmethod
    def _forget_missing(conn: sqlite3.Connection) -> None:
        """Drop what was kept for plugins that are no longer on disk (a mod removed)."""
        gone = [
            row[0]
            for row in conn.execute("SELECT DISTINCT plugin FROM records")
            if not Path(row[0]).exists()
        ]
        gone += [
            row[0]
            for row in conn.execute("SELECT DISTINCT plugin FROM key_stamps")
            if row[0] not in gone and not Path(row[0]).exists()
        ]
        gone += [
            row[0]
            for row in conn.execute("SELECT plugin FROM use_plugins")
            if row[0] not in gone and not Path(row[0]).exists()
        ]
        for table in ("records", "keys", "key_stamps"):
            conn.executemany(
                f"DELETE FROM {table} WHERE plugin=?",  # noqa: S608 - fixed table names
                [(g,) for g in gone],
            )
        for (plugin,) in [(g,) for g in gone]:
            RecordCache._drop_uses(conn, plugin)
        conn.executemany("DELETE FROM use_plugins WHERE plugin=?", [(g,) for g in gone])

    @staticmethod
    def _drop_uses(conn: sqlite3.Connection, plugin: str) -> None:
        """Remove what the use index holds of one plugin (the caller commits)."""
        conn.execute(
            "DELETE FROM uses WHERE rid IN (SELECT r.rid FROM use_records r"
            " JOIN use_plugins p ON p.pid = r.pid WHERE p.plugin=?)",
            (plugin,),
        )
        conn.execute(
            "DELETE FROM use_records WHERE pid IN (SELECT pid FROM use_plugins WHERE plugin=?)",
            (plugin,),
        )

    def get(self, plugin: Path, tag: str) -> list[Any] | None:
        """One plugin's cached records of a tag, when they are still current.

        Args:
            plugin: The plugin file.
            tag: The four-letter tag.

        Returns:
            The records, or None (never cached, or the plugin has changed since).
        """
        stamp = _stamp(plugin)
        if stamp is None:
            return None
        key, size, mtime = stamp
        with self._lock:
            conn = self._db()
            if conn is None:
                return None
            try:
                row = conn.execute(
                    "SELECT size, mtime_ns, data FROM records WHERE plugin=? AND tag=?",
                    (key, tag),
                ).fetchone()
                if row is None or row[0] != size or row[1] != mtime:
                    return None
                out = pickle.loads(zlib.decompress(row[2]))  # noqa: S301 - our own file
            except (sqlite3.Error, zlib.error, pickle.UnpicklingError, EOFError, ValueError):
                return None
        return out if isinstance(out, list) else None

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
        stamp = _stamp(plugin)
        if stamp is None:
            return
        key, size, mtime = stamp
        try:
            blob = zlib.compress(pickle.dumps(records, protocol=pickle.HIGHEST_PROTOCOL), 3)
        except (pickle.PicklingError, TypeError, AttributeError):
            return
        with self._lock:
            conn = self._db()
            if conn is None:
                return
            try:
                conn.execute(
                    "INSERT OR REPLACE INTO records VALUES (?, ?, ?, ?, ?)",
                    (key, tag, size, mtime, blob),
                )
                if keys is not None:
                    self._write_keys(conn, key, tag, size, mtime, keys)
                conn.commit()
                self._trim(conn, keep=tag)
            except sqlite3.Error as exc:
                LOG.warning("editor: record cache write failed: %s", exc)

    @staticmethod
    def _used(conn: sqlite3.Connection) -> int:
        """The bytes of the database in use (free pages not counted)."""
        pages = conn.execute("PRAGMA page_count").fetchone()[0]
        free = conn.execute("PRAGMA freelist_count").fetchone()[0]
        size = conn.execute("PRAGMA page_size").fetchone()[0]
        return int((pages - free) * size)

    def _trim(self, conn: sqlite3.Connection, keep: str = "") -> None:
        """Past the size limit, drop the record bodies of the tag that takes most.

        The key index stays (it is small and answers most lookups); a dropped tag is read
        from the plugins again when something asks for its records. The tag just written is
        spared while another can go, so a cache cannot empty the thing it was asked to keep.
        """
        dropped = 0
        while self._used(conn) > self.max_bytes:
            rows = conn.execute(
                "SELECT tag FROM records GROUP BY tag ORDER BY SUM(LENGTH(data)) DESC"
            ).fetchall()
            order = [r[0] for r in rows if r[0] != keep] or [r[0] for r in rows]
            if not order or dropped > 64:
                break
            conn.execute("DELETE FROM records WHERE tag=?", (order[0],))
            conn.commit()
            dropped += 1
            LOG.info(
                "editor: record cache over %d bytes: dropped the %s records",
                self.max_bytes,
                order[0],
            )
        if self._used(conn) > self.max_bytes:
            # The record bodies are gone and it is still over: the use-report index goes,
            # last, since it is the largest thing kept and the Use Report scans without it.
            conn.execute("DELETE FROM uses")
            conn.execute("DELETE FROM use_records")
            conn.execute("DELETE FROM use_plugins")
            conn.execute("DELETE FROM key_stamps WHERE tag=?", (_USES_TAG,))
            conn.commit()
            LOG.info("editor: record cache over %d bytes: dropped the use index", self.max_bytes)
            dropped += 1
        if dropped:
            self._shrink(conn)

    @staticmethod
    def _shrink(conn: sqlite3.Connection) -> None:
        """Give the disk back when more than half the file is free pages.

        Dropping bodies frees pages for reuse but SQLite does not shrink the file by
        itself; a ``VACUUM`` rewrites it. It holds the database for the time it takes, so
        it runs only after a trim (rare) and only when it would save at least half.
        """
        pages = conn.execute("PRAGMA page_count").fetchone()[0]
        free = conn.execute("PRAGMA freelist_count").fetchone()[0]
        if pages < 1024 or free * 2 <= pages:
            return
        try:
            conn.commit()
            conn.execute("VACUUM")
            conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")  # the file itself, not its log
        except sqlite3.Error as exc:
            LOG.warning("editor: record cache could not be compacted: %s", exc)

    def size_bytes(self) -> int:
        """The bytes of the database in use (0 when the cache is out)."""
        with self._lock:
            conn = self._db()
            if conn is None:
                return 0
            try:
                return self._used(conn)
            except sqlite3.Error:
                return 0

    @staticmethod
    def _write_keys(
        conn: sqlite3.Connection,
        plugin: str,
        tag: str,
        size: int,
        mtime: int,
        keys: Iterable[str],
    ) -> None:
        """Replace one plugin's indexed keys of a tag (the caller commits)."""
        conn.execute("DELETE FROM keys WHERE plugin=? AND tag=?", (plugin, tag))
        conn.executemany(
            "INSERT OR IGNORE INTO keys VALUES (?, ?, ?)", [(tag, k, plugin) for k in set(keys)]
        )
        conn.execute(
            "INSERT OR REPLACE INTO key_stamps VALUES (?, ?, ?, ?)", (plugin, tag, size, mtime)
        )

    def put_keys(self, plugin: Path, tag: str, keys: Iterable[str]) -> None:
        """Index a plugin's keys of a tag without keeping its records.

        Args:
            plugin: The plugin file.
            tag: The four-letter tag.
            keys: The records' keys (lower case).
        """
        stamp = _stamp(plugin)
        if stamp is None:
            return
        key, size, mtime = stamp
        with self._lock:
            conn = self._db()
            if conn is None:
                return
            try:
                self._write_keys(conn, key, tag, size, mtime, keys)
                conn.commit()
            except sqlite3.Error as exc:
                LOG.warning("editor: key index write failed: %s", exc)

    def _stale_of(self, plugins: Sequence[Path], tag: str) -> list[Path] | None:
        """The plugins whose keys are not indexed as they now are; None if the cache is out."""
        stamps = [(p, _stamp(p)) for p in plugins]
        have: dict[str, tuple[int, int]] = {}
        with self._lock:
            conn = self._db()
            if conn is None:
                return None
            try:
                keyed = [st[0] for _p, st in stamps if st is not None]
                for i in range(0, len(keyed), _CHUNK):
                    part = keyed[i : i + _CHUNK]
                    marks = ",".join("?" * len(part))
                    for row in conn.execute(
                        "SELECT plugin, size, mtime_ns FROM key_stamps"  # noqa: S608 - marks only
                        f" WHERE tag=? AND plugin IN ({marks})",
                        [tag, *part],
                    ):
                        have[row[0]] = (row[1], row[2])
            except sqlite3.Error:
                return None
        return [p for p, st in stamps if st is None or have.get(st[0]) != (st[1], st[2])]

    @staticmethod
    def _pack_uses(
        rows: Iterable[tuple[str, str, str, str, int]], last_rid: int
    ) -> tuple[list[tuple[int, str, str]], list[tuple[str, int, str, int]]]:
        """The rows of one plugin as records and (string, record) pairs.

        The rows come record by record (:func:`.usesindex.index_rows`), so a record is a run
        of rows with the same type and key; its definition row (count 0) only says that it
        exists, which the record row does. A string a record holds twice (as a value and as a
        script word, say) is one pair.
        """
        records: list[tuple[int, str, str]] = []
        merged: dict[tuple[str, int], list[Any]] = {}
        rid = last_rid
        current: tuple[str, str] | None = None
        for value, rtype, key, paths, n in rows:
            if (rtype, key) != current:
                rid += 1
                current = (rtype, key)
                records.append((rid, rtype, key))
            if n == 0:
                continue
            pair = merged.setdefault((value, rid), [[], 0])
            pair[0].extend(json.loads(paths))
            pair[1] += n
        strings = [
            (value, rid, "r" if p == ["references"] else json.dumps(p), n)
            for (value, rid), (p, n) in sorted(merged.items())
        ]
        return records, strings

    def put_uses(self, plugin: Path, rows: Iterable[tuple[str, str, str, str, int]]) -> None:
        """Index every string a plugin's records hold (the use-report index).

        Args:
            plugin: The plugin file.
            rows: ``(value, record type, record key, paths as JSON, count)`` (usesindex).
        """
        stamp = _stamp(plugin)
        if stamp is None:
            return
        key, size, mtime = stamp
        with self._lock:
            conn = self._db()
            if conn is None:
                return
            try:
                conn.execute("INSERT OR IGNORE INTO use_plugins (plugin) VALUES (?)", (key,))
                pid = conn.execute("SELECT pid FROM use_plugins WHERE plugin=?", (key,)).fetchone()[
                    0
                ]
                self._drop_uses(conn, key)
                records, strings = self._pack_uses(
                    rows,
                    int(
                        conn.execute("SELECT COALESCE(MAX(rid), 0) FROM use_records").fetchone()[0]
                    ),
                )
                conn.executemany(
                    "INSERT INTO use_records VALUES (?, ?, ?, ?)",
                    [(rid, pid, t, k) for rid, t, k in records],
                )
                conn.executemany("INSERT INTO uses VALUES (?, ?, ?, ?)", strings)
                conn.execute(
                    "INSERT OR REPLACE INTO key_stamps VALUES (?, ?, ?, ?)",
                    (key, _USES_TAG, size, mtime),
                )
                conn.commit()
                self._trim(conn)
            except sqlite3.Error as exc:
                LOG.warning("editor: use index write failed: %s", exc)

    def missing_uses(self, plugins: Sequence[Path]) -> list[Path]:
        """The plugins whose strings are not indexed as they now are (all if the cache is out)."""
        return self.missing_keys(plugins, _USES_TAG)

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
        stale = self._stale_of(plugins, _USES_TAG)
        if stale is None or stale:
            return None
        order = {st[0]: i for i, p in enumerate(plugins) if (st := _stamp(p)) is not None}
        by_name = {st[0]: p for p in plugins if (st := _stamp(p)) is not None}
        out: list[tuple[int, int, Path, str, str, list[str], int]] = []
        with self._lock:
            conn = self._db()
            if conn is None:
                return None
            try:
                for plug, rtype, key, paths, n, rid in conn.execute(
                    "SELECT p.plugin, r.rtype, r.key, u.paths, u.n, r.rid FROM uses u"
                    " JOIN use_records r ON r.rid = u.rid"
                    " JOIN use_plugins p ON p.pid = r.pid WHERE u.value=?",
                    (value,),
                ):
                    if plug in order:
                        out.append(
                            (
                                order[plug],
                                rid,
                                by_name[plug],
                                rtype,
                                key,
                                ["references"] if paths == "r" else json.loads(paths),
                                n,
                            )
                        )
            except (sqlite3.Error, ValueError):
                return None
        out.sort(key=lambda r: (r[0], r[1]))
        return [(p, t, k, paths, n) for _o, _r, p, t, k, paths, n in out]

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
        stale = self._stale_of(plugins, _USES_TAG)
        if stale is None or stale:
            return None
        order = {st[0]: i for i, p in enumerate(plugins) if (st := _stamp(p)) is not None}
        by_name = {st[0]: p for p in plugins if (st := _stamp(p)) is not None}
        wanted = set(records)
        values = sorted({key for _t, key in wanted})
        found: dict[tuple[str, str], set[str]] = {}
        with self._lock:
            conn = self._db()
            if conn is None:
                return None
            try:
                for i in range(0, len(values), _CHUNK):
                    part = values[i : i + _CHUNK]
                    marks = ",".join("?" * len(part))
                    for plug, rtype, key in conn.execute(
                        "SELECT p.plugin, r.rtype, r.key FROM use_records r"  # noqa: S608 - marks only
                        " JOIN use_plugins p ON p.pid = r.pid"
                        f" WHERE lower(r.key) IN ({marks})",
                        part,
                    ):
                        pair = (rtype, key.lower())
                        if pair in wanted and plug in order:
                            found.setdefault(pair, set()).add(plug)
            except sqlite3.Error:
                return None
        return {
            pair: [by_name[n] for n in sorted(names, key=order.__getitem__)]
            for pair, names in found.items()
        }

    def keys_current(self, plugin: Path, tag: str) -> bool:
        """Whether a plugin's keys of a tag are indexed and the plugin is unchanged since."""
        return self._stale_of([plugin], tag) == []

    def missing_keys(self, plugins: Sequence[Path], tag: str) -> list[Path]:
        """The plugins whose keys of a tag still have to be indexed (all, if the cache is out)."""
        stale = self._stale_of(plugins, tag)
        return list(plugins) if stale is None else stale

    def holders(self, plugins: Sequence[Path], tag: str, key: str) -> list[Path] | None:
        """Which of these plugins have a key, from the index.

        Args:
            plugins: The load order's plugin files.
            tag: The four-letter tag.
            key: The record's key, lower case.

        Returns:
            The plugins that have it (in the order given), or None when the index cannot
            answer: some plugin is not indexed as it now is, or the cache is out.
        """
        stale = self._stale_of(plugins, tag)
        if stale is None or stale:
            return None
        stamped = [(p, st[0]) for p in plugins if (st := _stamp(p)) is not None]
        found: set[str] = set()
        with self._lock:
            conn = self._db()
            if conn is None:
                return None
            try:
                listed = [name for _p, name in stamped]
                for i in range(0, len(listed), _CHUNK):
                    part = listed[i : i + _CHUNK]
                    marks = ",".join("?" * len(part))
                    found.update(
                        row[0]
                        for row in conn.execute(
                            "SELECT plugin FROM keys"  # noqa: S608 - marks only
                            f" WHERE tag=? AND key=? AND plugin IN ({marks})",
                            [tag, key, *part],
                        )
                    )
            except sqlite3.Error:
                return None
        return [p for p, name in stamped if name in found]

    def all_keys(self, plugins: Sequence[Path], tag: str) -> set[str] | None:
        """Every key of a tag over these plugins, from the index.

        Args:
            plugins: The load order's plugin files.
            tag: The four-letter tag.

        Returns:
            The keys (lower case), or None when the index cannot answer (some plugin is
            not indexed as it now is, or the cache is out).
        """
        stale = self._stale_of(plugins, tag)
        if stale is None or stale:
            return None
        names = [st[0] for p in plugins if (st := _stamp(p)) is not None]
        out: set[str] = set()
        with self._lock:
            conn = self._db()
            if conn is None:
                return None
            try:
                for i in range(0, len(names), _CHUNK):
                    part = names[i : i + _CHUNK]
                    marks = ",".join("?" * len(part))
                    out.update(
                        row[0]
                        for row in conn.execute(
                            "SELECT DISTINCT key FROM keys"  # noqa: S608 - marks only
                            f" WHERE tag=? AND plugin IN ({marks})",
                            [tag, *part],
                        )
                    )
            except sqlite3.Error:
                return None
        return out

    @staticmethod
    def load_order_signature(plugins: Sequence[Path]) -> str | None:
        """One text for a load order: which plugins, in what order, as they now are.

        Args:
            plugins: The plugin files, in load order.

        Returns:
            A hash of every plugin's path, size and time, or None when one cannot be
            looked at (nothing is then cached under it).
        """
        digest = hashlib.sha256()
        for plugin in plugins:
            stamp = _stamp(plugin)
            if stamp is None:
                return None
            digest.update(repr(stamp).encode())
        return digest.hexdigest()

    def get_report(self, sig: str, kind: str, key: str) -> list[Any] | None:
        """A report kept for a load order (:meth:`load_order_signature`).

        Args:
            sig: The load order's signature.
            kind: What the report is (``uses:Weapon``).
            key: What it is about (a record's key, lower case).

        Returns:
            What was kept, or None.
        """
        with self._lock:
            conn = self._db()
            if conn is None:
                return None
            try:
                row = conn.execute(
                    "SELECT data FROM reports WHERE sig=? AND kind=? AND key=?", (sig, kind, key)
                ).fetchone()
                out = None if row is None else pickle.loads(zlib.decompress(row[0]))  # noqa: S301
            except (sqlite3.Error, zlib.error, pickle.UnpicklingError, EOFError, ValueError):
                return None
        return out if isinstance(out, list) else None

    def put_report(self, sig: str, kind: str, key: str, value: list[Any]) -> None:
        """Keep a report for a load order; the oldest are dropped past a limit.

        Args:
            sig: The load order's signature.
            kind: What the report is.
            key: What it is about.
            value: The report (anything that pickles).
        """
        try:
            blob = zlib.compress(pickle.dumps(value, protocol=pickle.HIGHEST_PROTOCOL), 3)
        except (pickle.PicklingError, TypeError, AttributeError):
            return
        with self._lock:
            conn = self._db()
            if conn is None:
                return
            try:
                conn.execute(
                    "INSERT OR REPLACE INTO reports VALUES (?, ?, ?, ?)", (sig, kind, key, blob)
                )
                conn.execute("DELETE FROM reports WHERE sig != ?", (sig,))
                conn.execute(
                    "DELETE FROM reports WHERE rowid IN"
                    " (SELECT rowid FROM reports ORDER BY rowid DESC LIMIT -1 OFFSET ?)",
                    (_REPORTS_KEPT,),
                )
                conn.commit()
            except sqlite3.Error as exc:
                LOG.warning("editor: report cache write failed: %s", exc)

    def clear(self) -> None:
        """Forget everything cached."""
        with self._lock:
            conn = self._db()
            if conn is None:
                return
            try:
                conn.execute("DELETE FROM records")
                conn.execute("DELETE FROM keys")
                conn.execute("DELETE FROM key_stamps")
                conn.execute("DELETE FROM reports")
                conn.commit()
            except sqlite3.Error:
                return

    def stats(self) -> dict[str, int]:
        """What the cache holds: the file's pages and rows per table (0s when it is out).

        Returns:
            ``pages``, ``free`` (pages), ``page_size``, ``used`` (bytes), and a ``rows_<table>``
            for each table.
        """
        out: dict[str, int] = {}
        with self._lock:
            conn = self._db()
            if conn is None:
                return out
            try:
                out["pages"] = conn.execute("PRAGMA page_count").fetchone()[0]
                out["free"] = conn.execute("PRAGMA freelist_count").fetchone()[0]
                out["page_size"] = conn.execute("PRAGMA page_size").fetchone()[0]
                out["used"] = (out["pages"] - out["free"]) * out["page_size"]
                for table in (
                    "records",
                    "keys",
                    "key_stamps",
                    "uses",
                    "use_records",
                    "use_plugins",
                    "reports",
                ):
                    query = "SELECT COUNT(*) FROM " + table  # noqa: S608 - fixed names above
                    out[f"rows_{table}"] = conn.execute(query).fetchone()[0]
            except sqlite3.Error:
                return out
        return out

    def compact(self) -> bool:
        """Give the disk back when more than half the file is free pages.

        Returns:
            Whether the file was rewritten.
        """
        with self._lock:
            conn = self._db()
            if conn is None:
                return False
            before = conn.execute("PRAGMA page_count").fetchone()[0]
            self._shrink(conn)
            return bool(conn.execute("PRAGMA page_count").fetchone()[0] < before)

    def close(self) -> None:
        """Close the database (it reopens on next use), compacting it first if it is mostly free.

        Rewrites that leave free pages behind (a plugin indexed again, a trim) never shrink the
        file by themselves; closing is when nothing is waiting on the cache, so a file that is
        more than half free pages is rewritten here (a second or two for a few hundred MB).
        """
        with self._lock:
            if self._conn is not None:
                try:
                    self._shrink(self._conn)
                    self._conn.close()
                except sqlite3.Error:
                    # Closing is cleanup only: a connection that will not close cleanly
                    # (the file went away, say) has nothing left to lose or to report.
                    pass
                self._conn = None

    def __enter__(self) -> RecordCache:
        """Use the cache in a ``with``: it is closed on the way out."""
        return self

    def __exit__(self, *_exc: object) -> None:
        """Close the database."""
        self.close()

    def __del__(self) -> None:
        """Close the database when the cache is dropped, so no connection is left open."""
        if getattr(self, "_lock", None) is not None:
            self.close()


__all__ = ["RecordCache"]
