r"""Print what is in the editor's record cache and how much room each part takes.

Usage (from the repo root, with the app closed or open - it only reads):
    python tools/cache_stats.py [path\to\wraithguard_record_cache.sqlite3]

With no path it looks next to wraithguard_toolkit_gui.py.
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path


def main() -> int:
    """Print the statistics.

    Returns:
        0, or 1 when there is no cache.
    """
    here = Path(__file__).resolve().parent.parent
    db = Path(sys.argv[1]) if len(sys.argv) > 1 else here / "wraithguard_record_cache.sqlite3"
    if not db.exists():
        print(f"no cache at {db}")
        return 1
    print(f"{db}  file: {db.stat().st_size / 1e6:.1f} MB")
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    try:
        pages = con.execute("PRAGMA page_count").fetchone()[0]
        free = con.execute("PRAGMA freelist_count").fetchone()[0]
        size = con.execute("PRAGMA page_size").fetchone()[0]
        print(
            f"pages {pages}, free {free} ({free / max(pages, 1):.0%}), in use {(pages - free) * size / 1e6:.1f} MB"
        )
        print("meta:", dict(con.execute("SELECT k, v FROM meta")))
        try:
            rows = con.execute(
                "SELECT name, SUM(pgsize) FROM dbstat GROUP BY name ORDER BY 2 DESC"
            ).fetchall()
            print("\nspace by table/index (MB):")
            for name, nbytes in rows:
                print(f"  {name:28s} {nbytes / 1e6:9.1f}")
        except sqlite3.Error:
            print("\n(this SQLite has no dbstat; row counts only)")
        print("\nrows:")
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
            try:
                n = con.execute(query).fetchone()[0]
            except sqlite3.Error:
                continue
            print(f"  {table:12s} {n:>12,}")
        print("\nrecord bodies by tag (MB, plugins):")
        for tag, nbytes, plugins in con.execute(
            "SELECT tag, SUM(LENGTH(data)), COUNT(*) FROM records GROUP BY tag ORDER BY 2 DESC LIMIT 15"
        ):
            print(f"  {tag:8s} {nbytes / 1e6:9.1f}  {plugins}")
        print(
            "\nuse-index plugins indexed:",
            con.execute("SELECT COUNT(*) FROM key_stamps WHERE tag='~USE'").fetchone()[0],
        )
        top = con.execute(
            "SELECT p.plugin, COUNT(*) FROM uses u JOIN use_records r ON r.rid=u.rid"
            " JOIN use_plugins p ON p.pid=r.pid GROUP BY r.pid ORDER BY 2 DESC LIMIT 5"
        ).fetchall()
        print("biggest plugins in the use index (rows):")
        for plugin, n in top:
            print(f"  {n:>10,}  {Path(plugin).name}")
    finally:
        con.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
