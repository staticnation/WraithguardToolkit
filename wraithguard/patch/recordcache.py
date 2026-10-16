"""A cache of plugin records on disk, so the editor reads a load order once.

Reading one tag (the cells, say) out of hundreds of plugins is minutes of file reading the
first time and nothing the second. Each entry is one plugin's records of one tag, valid
while that plugin's size and modification time are the ones it was read at; a plugin that
changed is simply read again. The cache is only ever a shortcut: any failure of it (a
locked, corrupt or missing database) is a miss, never an error in the editor.

Copyright (c) 2026 StaticNation, GPL-2.0.
"""

from __future__ import annotations

import contextlib
import logging
import os
import pickle
import sqlite3
import threading
import zlib
from pathlib import Path
from typing import Any, Final

LOG = logging.getLogger(__name__)

#: Bumped when what is stored changes shape; an older database is emptied, not read.
SCHEMA: Final = 1


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

    def __init__(self, path: Path) -> None:
        """Remember where the database is; it is opened on first use.

        Args:
            path: The database file (created when needed).
        """
        self.path = path
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
            if row is None or row[0] != SCHEMA:
                conn.execute("DROP TABLE IF EXISTS records")
                conn.execute("INSERT OR REPLACE INTO meta VALUES ('schema', ?)", (SCHEMA,))
            conn.execute(
                "CREATE TABLE IF NOT EXISTS records (plugin TEXT NOT NULL, tag TEXT NOT NULL,"
                " size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, data BLOB NOT NULL,"
                " PRIMARY KEY (plugin, tag))"
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
        conn.executemany("DELETE FROM records WHERE plugin=?", [(g,) for g in gone])

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

    def put(self, plugin: Path, tag: str, records: list[Any]) -> None:
        """Keep a plugin's records of a tag, as the plugin is now.

        Args:
            plugin: The plugin file.
            tag: The four-letter tag.
            records: What was read.
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
                conn.commit()
            except sqlite3.Error as exc:
                LOG.warning("editor: record cache write failed: %s", exc)

    def clear(self) -> None:
        """Forget everything cached."""
        with self._lock:
            conn = self._db()
            if conn is None:
                return
            try:
                conn.execute("DELETE FROM records")
                conn.commit()
            except sqlite3.Error:
                return

    def close(self) -> None:
        """Close the database (it reopens on next use)."""
        with self._lock:
            if self._conn is not None:
                try:
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
