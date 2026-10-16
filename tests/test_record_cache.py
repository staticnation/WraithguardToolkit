"""The editor's on-disk record cache: a plugin is read once, and again only when it changes."""

from __future__ import annotations

import os
from typing import TYPE_CHECKING, Any

from wraithguard.patch.editor import EditorSession
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.recordcache import RecordCache

if TYPE_CHECKING:
    from pathlib import Path

CELL = {"type": "Cell", "name": "Vault", "data": {"grid": (0, 0)}}


def _plugin(tmp_path: Path, name: str = "a.esm", body: bytes = b"x" * 10) -> Path:
    p = tmp_path / name
    p.write_bytes(body)
    return p


def test_a_record_survives_a_round_trip_unchanged(tmp_path: Path) -> None:
    cache = RecordCache(tmp_path / "c.sqlite3")
    plugin = _plugin(tmp_path)
    assert cache.get(plugin, "CELL") is None
    cache.put(plugin, "CELL", [CELL])
    assert cache.get(plugin, "CELL") == [CELL]  # tuples stay tuples
    assert RecordCache(tmp_path / "c.sqlite3").get(plugin, "CELL") == [CELL]  # across runs


def test_a_plugin_that_changed_is_a_miss(tmp_path: Path) -> None:
    cache = RecordCache(tmp_path / "c.sqlite3")
    plugin = _plugin(tmp_path)
    cache.put(plugin, "CELL", [CELL])
    plugin.write_bytes(b"x" * 11)  # another size
    assert cache.get(plugin, "CELL") is None
    cache.put(plugin, "CELL", [CELL])
    st = plugin.stat()
    os.utime(plugin, ns=(st.st_atime_ns, st.st_mtime_ns + 5_000_000_000))  # another time
    assert cache.get(plugin, "CELL") is None


def test_a_broken_database_is_a_miss_not_an_error(tmp_path: Path) -> None:
    db = tmp_path / "c.sqlite3"
    db.write_bytes(b"this is not a database" * 50)
    cache = RecordCache(db)
    plugin = _plugin(tmp_path)
    assert cache.get(plugin, "CELL") is None
    cache.put(plugin, "CELL", [CELL])  # does not raise


def test_a_second_session_reads_nothing_the_first_read(tmp_path: Path) -> None:
    plugins = [(n, _plugin(tmp_path, n)) for n in ("a.esm", "b.esp")]
    reads: list[str] = []

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        reads.append(path.name)
        return [CELL]

    db = tmp_path / "c.sqlite3"
    first = EditorSession(plugins, PatchQueue(), read=read, disk_cache=RecordCache(db))
    first.warm("CELL")
    assert sorted(reads) == ["a.esm", "b.esp"]
    reads.clear()
    second = EditorSession(plugins, PatchQueue(), read=read, disk_cache=RecordCache(db))
    second.warm("CELL")
    assert reads == []  # all from the cache


def test_a_removed_plugin_is_forgotten_when_the_cache_opens(tmp_path: Path) -> None:
    db = tmp_path / "c.sqlite3"
    keep, drop = _plugin(tmp_path, "keep.esp"), _plugin(tmp_path, "drop.esp")
    cache = RecordCache(db)
    cache.put(keep, "CELL", [CELL])
    cache.put(drop, "CELL", [CELL])
    cache.close()
    drop.unlink()
    again = RecordCache(db)
    assert again.get(keep, "CELL") == [CELL]
    import sqlite3
    from contextlib import closing

    with closing(sqlite3.connect(db)) as raw:
        rows = raw.execute("SELECT COUNT(*) FROM records").fetchone()[0]
    assert rows == 1  # the removed plugin's entry is gone, not just unreadable


def test_a_moved_reference_is_found_from_the_cached_index(tmp_path: Path) -> None:
    def ref(mast: int, refr: int, **more: Any) -> dict[str, Any]:
        return {"mast_index": mast, "refr_index": refr, "id": "rock", **more}

    def ext(*refs: dict[str, Any], grid: tuple[int, int]) -> dict[str, Any]:
        return {
            "type": "Cell",
            "flags": "",
            "id": "",
            "name": "",
            "data": {"flags": "", "grid": list(grid)},
            "references": list(refs),
        }

    plugin = _plugin(tmp_path, "Morrowind.esm")
    records = [{"type": "Header", "masters": []}, ext(ref(0, 5), grid=(0, 0)), ext(grid=(1, 0))]
    walks: list[str] = []

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        walks.append(tag)
        want = {"TES3": "Header", "CELL": "Cell"}[tag]
        return [r for r in records if r["type"] == want]

    db = tmp_path / "c.sqlite3"
    for _run in range(2):
        s = EditorSession(
            [("Morrowind.esm", plugin)], PatchQueue(), read=read, disk_cache=RecordCache(db)
        )
        found = s.find_ref("(1, 0)", "Morrowind.esm", 5, ["Morrowind.esm"])
        assert found is not None and found.cell == "(0, 0)"
    assert RecordCache(db).get(plugin, "HOME") == [(5, "morrowind.esm", "(0, 0)")]
    assert walks.count("CELL") == 1  # the second run read the cells from the cache
