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


def _cell(name: str = "", grid: tuple[int, int] = (0, 0), interior: bool = False) -> dict[str, Any]:
    flags = "IS_INTERIOR" if interior else ""
    return {"type": "Cell", "name": name, "data": {"grid": grid, "flags": flags}}


def test_the_key_index_answers_which_plugins_have_a_key(tmp_path: Path) -> None:
    cache = RecordCache(tmp_path / "c.sqlite3")
    a, b = _plugin(tmp_path, "a.esm"), _plugin(tmp_path, "b.esp")
    assert cache.holders([a, b], "CELL", "room") is None  # nothing indexed: it cannot say
    cache.put_keys(a, "CELL", ["room", "(0, 0)"])
    assert cache.holders([a, b], "CELL", "room") is None  # one plugin still not indexed
    cache.put_keys(b, "CELL", ["(1, 1)"])
    assert cache.holders([a, b], "CELL", "room") == [a]
    assert cache.holders([a, b], "CELL", "(1, 1)") == [b]
    assert cache.holders([a, b], "CELL", "(9, 9)") == []  # indexed, and not there
    assert cache.holders([a, b], "LAND", "(1, 1)") is None  # another tag is not indexed
    assert RecordCache(tmp_path / "c.sqlite3").holders([b, a], "CELL", "room") == [a]  # kept


def test_a_plugin_that_changed_is_not_answered_for_from_the_index(tmp_path: Path) -> None:
    cache = RecordCache(tmp_path / "c.sqlite3")
    a = _plugin(tmp_path, "a.esm")
    cache.put_keys(a, "CELL", ["room"])
    assert cache.holders([a], "CELL", "room") == [a]
    a.write_bytes(b"x" * 11)
    assert cache.holders([a], "CELL", "room") is None
    assert cache.missing_keys([a], "CELL") == [a]
    assert not cache.keys_current(a, "CELL")
    cache.put_keys(a, "CELL", [])
    assert cache.holders([a], "CELL", "room") == []  # the new keys replaced the old


def test_put_with_keys_indexes_them_too(tmp_path: Path) -> None:
    cache = RecordCache(tmp_path / "c.sqlite3")
    a = _plugin(tmp_path, "a.esm")
    cache.put(a, "CELL", [CELL], keys=["vault"])
    assert cache.holders([a], "CELL", "vault") == [a]
    cache.clear()
    assert cache.holders([a], "CELL", "vault") is None


def test_a_removed_plugins_keys_are_forgotten(tmp_path: Path) -> None:
    db = tmp_path / "c.sqlite3"
    gone = _plugin(tmp_path, "gone.esp")
    RecordCache(db).put_keys(gone, "CELL", ["room"])
    gone.unlink()
    here = _plugin(tmp_path, "a.esm")
    cache = RecordCache(db)
    cache.put_keys(here, "CELL", ["x"])  # opens the database: the missing plugin is dropped
    import sqlite3

    rows = sqlite3.connect(db).execute("SELECT COUNT(*) FROM keys WHERE key='room'").fetchone()
    assert rows[0] == 0


def _session(tmp_path: Path, contents: dict[str, list[dict[str, Any]]], reads: list[str]):
    # A second session over the same files: they are not written again (that would be a change).
    plugins = [
        (n, tmp_path / n if (tmp_path / n).exists() else _plugin(tmp_path, n)) for n in contents
    ]

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        reads.append(f"{path.name}:{tag}")
        return contents[path.name] if tag == "CELL" else []

    return EditorSession(
        plugins, PatchQueue(), None, read=read, disk_cache=RecordCache(tmp_path / "c.sqlite3")
    )


def test_a_name_is_checked_free_by_the_index_without_reading_plugins(tmp_path: Path) -> None:
    reads: list[str] = []
    s = _session(
        tmp_path,
        {"a.esm": [_cell("Vault", interior=True)], "b.esp": [_cell(grid=(2, 3))]},
        reads,
    )
    assert s.ensure_index("CELL") == 2  # the long part, once
    assert sorted(reads) == ["a.esm:CELL", "b.esp:CELL"]
    reads.clear()
    assert s.in_load_order("CELL", "Vault") and s.in_load_order("CELL", "(2, 3)")
    assert not s.in_load_order("CELL", "Brand new room")
    assert s.holders("CELL", "vault") == ["a.esm"]
    assert s._cell_plugins("(2, 3)", None) == ("b.esp",)
    assert reads == []  # every answer was a query
    assert s.ensure_index("CELL") == 0  # nothing left to index


def test_a_second_session_finds_the_index_ready(tmp_path: Path) -> None:
    contents = {"a.esm": [_cell("Vault", interior=True)]}
    first: list[str] = []
    _session(tmp_path, contents, first).ensure_index("CELL")
    second: list[str] = []
    s = _session(tmp_path, contents, second)
    assert s.in_load_order("CELL", "Vault")
    assert second == []  # a new session reads nothing


def test_without_an_index_a_lookup_reads_and_still_answers(tmp_path: Path) -> None:
    reads: list[str] = []
    s = _session(tmp_path, {"a.esm": [_cell("Vault", interior=True)]}, reads)
    assert s.in_load_order("CELL", "Vault")  # not indexed: falls back to reading
    assert reads == ["a.esm:CELL"]
    assert not s.in_load_order("CELL", "Elsewhere")


def test_the_patchs_own_records_count_as_present(tmp_path: Path) -> None:
    s = _session(tmp_path, {"a.esm": []}, [])
    s.new_cell("Made here")
    assert s.in_load_order("CELL", "Made here")


def test_background_indexing_fills_the_index_and_stops_on_close(tmp_path: Path) -> None:
    reads: list[str] = []
    s = _session(tmp_path, {"a.esm": [_cell("Vault", interior=True)]}, reads)
    s.start_indexing(("CELL",))
    assert s._indexer is not None
    s._indexer.join(timeout=10)
    assert not s._indexer.is_alive()
    assert s.holders("CELL", "vault") == ["a.esm"]
    s.close()
    s2 = _session(tmp_path, {"b.esm": [_cell("Other", interior=True)]}, [])
    s2.close()  # closed before it starts: it indexes nothing
    assert s2.ensure_index("CELL") == 0


def test_indexing_on_several_threads_gives_the_same_index(tmp_path: Path) -> None:
    names = [f"p{i}.esp" for i in range(8)]
    contents = {
        n: [_cell(f"Room {i}", interior=True), _cell(grid=(i, i))] for i, n in enumerate(names)
    }
    reads: list[str] = []
    s = _session(tmp_path, contents, reads)
    assert s.ensure_index("CELL", workers=4) == 8
    assert sorted(reads) == sorted(f"{n}:CELL" for n in names)  # each read once
    for i, n in enumerate(names):
        assert s.holders("CELL", f"Room {i}") == [n]
        assert s.holders("CELL", f"({i}, {i})") == [n]
    assert s.ensure_index("CELL", workers=4) == 0


def test_two_threads_indexing_at_once_leave_a_whole_index(tmp_path: Path) -> None:
    import threading

    names = [f"p{i}.esp" for i in range(6)]
    contents = {n: [_cell(f"Room {i}", interior=True)] for i, n in enumerate(names)}
    s = _session(tmp_path, contents, [])
    threads = [threading.Thread(target=s.ensure_index, args=("CELL", 3)) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=20)
    assert all(s.holders("CELL", f"Room {i}") == [n] for i, n in enumerate(names))
    # Starting the background indexer twice from two threads starts one.
    first = [threading.Thread(target=s.start_indexing, args=(("CELL",),)) for _ in range(4)]
    for t in first:
        t.start()
    for t in first:
        t.join(timeout=20)
    assert s._indexer is not None
    s._indexer.join(timeout=10)


def test_a_closed_session_stops_indexing(tmp_path: Path) -> None:
    contents = {f"p{i}.esp": [_cell(f"Room {i}", interior=True)] for i in range(4)}
    reads: list[str] = []
    s = _session(tmp_path, contents, reads)
    s.close()
    assert s.ensure_index("CELL", workers=2) == 0
    assert reads == []
