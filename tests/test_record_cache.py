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
    from contextlib import closing

    with closing(sqlite3.connect(db)) as raw:
        rows = raw.execute("SELECT COUNT(*) FROM keys WHERE key='room'").fetchone()
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


def test_a_cache_made_by_another_build_of_the_reader_is_emptied(tmp_path: Path) -> None:
    db = tmp_path / "c.sqlite3"
    plugin = _plugin(tmp_path)
    with RecordCache(db, parser="reader-1") as old:
        old.put(plugin, "CELL", [CELL], ["vault"])
        assert old.get(plugin, "CELL") == [CELL]
    with RecordCache(db, parser="reader-1") as same:
        assert same.get(plugin, "CELL") == [CELL]  # the same build: kept
        assert same.keys_current(plugin, "CELL")
    with RecordCache(db, parser="reader-2") as new:
        assert new.get(plugin, "CELL") is None  # another build: not trusted
        assert not new.keys_current(plugin, "CELL")  # the key index with it


def test_a_reader_signature_of_digits_is_not_mistaken_for_a_change(tmp_path: Path) -> None:
    db = tmp_path / "c.sqlite3"
    plugin = _plugin(tmp_path)
    with RecordCache(db, parser="12345") as first:
        first.put(plugin, "CELL", [CELL])
    with RecordCache(db, parser="12345") as again:
        assert again.get(plugin, "CELL") == [CELL]


def test_dialogue_and_masters_come_from_the_cache_in_a_second_session(tmp_path: Path) -> None:
    plugin = _plugin(tmp_path, "a.esm")
    topic = {"type": "Dialogue", "id": "hello", "dialogue_type": "Topic"}
    reads: list[str] = []

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        reads.append(tag)
        return [{"type": "Header", "masters": [["Morrowind.esm", 1]]}] if tag == "TES3" else []

    def read_dialogue(path: Path) -> list[dict[str, Any]]:
        reads.append("DIAL+INFO")
        return [topic]

    db = tmp_path / "c.sqlite3"
    for _run in range(2):
        with RecordCache(db) as cache:
            s = EditorSession(
                [("a.esm", plugin)],
                PatchQueue(),
                read=read,
                read_dialogue=read_dialogue,
                disk_cache=cache,
            )
            assert s._dialogue("a.esm") == [topic]
            s._masters_of("a.esm")
    assert sorted(reads) == ["DIAL+INFO", "TES3"]  # read once, not once per run


def _weapon_session(tmp_path: Path, plugin: Path, calls: dict[str, int], cache: RecordCache):
    sword = {"type": "Weapon", "id": "iron sword", "data": {"weight": 1.0}}
    holder = {"type": "LeveledItem", "id": "l_loot", "items": [["iron sword", 1]]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        calls["read"] += 1
        return [sword] if tag == "WEAP" else []

    def read_all(path: Path) -> list[dict[str, Any]]:
        calls["all"] += 1
        return [sword, holder]

    return EditorSession(
        [("a.esm", plugin)], PatchQueue(), read=read, read_all=read_all, disk_cache=cache
    )


def test_a_use_report_is_kept_until_a_plugin_changes(tmp_path: Path) -> None:
    plugin = _plugin(tmp_path, "a.esm")
    db = tmp_path / "c.sqlite3"
    calls = {"read": 0, "all": 0}
    answers = []
    for _run in range(2):
        with RecordCache(db) as cache:
            s = _weapon_session(tmp_path, plugin, calls, cache)
            found = s.find("WEAP", "iron sword")
            assert found is not None
            answers.append(s.uses(found))
    assert calls["all"] == 1  # the second run scanned nothing
    assert answers[0] == answers[1]
    assert [u["key"] for u in answers[0]["uses"]] == ["l_loot"]
    plugin.write_bytes(b"x" * 99)  # the plugin changed: the report is not the same load order's
    with RecordCache(db) as cache:
        s = _weapon_session(tmp_path, plugin, calls, cache)
        found = s.find("WEAP", "iron sword")
        assert found is not None
        s.uses(found)
    assert calls["all"] == 2


def test_the_ids_of_a_tag_come_from_the_index_in_a_second_session(tmp_path: Path) -> None:
    plugin = _plugin(tmp_path, "a.esm")
    db = tmp_path / "c.sqlite3"
    calls = {"read": 0, "all": 0}
    for run in range(2):
        with RecordCache(db) as cache:
            s = _weapon_session(tmp_path, plugin, calls, cache)
            assert s.plugin_ids("WEAP") == {"iron sword"}
        if run == 0:
            assert calls["read"] >= 1
    first = calls["read"]
    with RecordCache(db) as cache:
        s = _weapon_session(tmp_path, plugin, calls, cache)
        assert s.plugin_ids("WEAP") == {"iron sword"}
    assert calls["read"] == first  # a later run read no plugin for its ids


def test_a_load_order_signature_follows_order_and_content(tmp_path: Path) -> None:
    a, b = _plugin(tmp_path, "a.esm"), _plugin(tmp_path, "b.esp")
    sig = RecordCache.load_order_signature
    assert sig([a, b]) == sig([a, b])
    assert sig([a, b]) != sig([b, a])  # which wins depends on the order
    assert sig([a, tmp_path / "gone.esp"]) is None  # a plugin that cannot be seen: not cached


def test_reports_of_an_older_load_order_go_when_a_new_one_is_kept(tmp_path: Path) -> None:
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        cache.put_report("old", "uses:Weapon", "sword", [("a.esm", "Weapon", "x", [], 1, True)])
        assert cache.get_report("old", "uses:Weapon", "sword") is not None
        cache.put_report("new", "uses:Weapon", "sword", [])
        assert cache.get_report("old", "uses:Weapon", "sword") is None
        assert cache.get_report("new", "uses:Weapon", "sword") == []


def test_reports_are_capped_by_count_not_by_plugin_count(tmp_path: Path) -> None:
    import wraithguard.patch.recordcache as rc

    with RecordCache(tmp_path / "c.sqlite3") as cache:
        for i in range(rc._REPORTS_KEPT + 5):
            cache.put_report("sig", "uses:Weapon", f"k{i}", [])
        assert cache.get_report("sig", "uses:Weapon", "k0") is None  # the oldest went
        assert cache.get_report("sig", "uses:Weapon", f"k{rc._REPORTS_KEPT + 4}") == []


def test_several_tags_are_indexed_with_one_pass_per_plugin_group(tmp_path: Path) -> None:
    contents = {"a.esm": [_cell("Vault", interior=True)], "b.esp": [_cell(grid=(2, 3))]}
    s = _session(tmp_path, contents, [])
    assert s.ensure_index_many(("CELL", "GLOB", "SCPT"), workers=1) == 2
    assert s.holders("CELL", "vault") == ["a.esm"]
    assert s.holders("GLOB", "anything") == []  # indexed (empty), so the answer is "none"
    assert s.holders("SCPT", "anything") == []
    assert s.ensure_index_many(("CELL", "GLOB", "SCPT"), workers=1) == 0  # nothing left to read


def test_verify_asks_the_index_for_ids_after_one_pass(tmp_path: Path) -> None:
    reads: list[str] = []
    s = _session(tmp_path, {"a.esm": [_cell("Vault", interior=True)]}, reads)
    s.ensure_index_many(("CELL",), workers=1)
    reads.clear()
    assert s.plugin_ids("CELL") == {"vault"}
    assert reads == []


def test_over_its_size_the_cache_drops_the_largest_tags_bodies_and_keeps_the_keys(
    tmp_path: Path,
) -> None:
    import os

    a, b = _plugin(tmp_path, "a.esm"), _plugin(tmp_path, "b.esp")
    with RecordCache(tmp_path / "c.sqlite3", max_bytes=300_000) as cache:
        cache.put(a, "LAND", [os.urandom(200_000)], keys=["(0, 0)"])
        assert cache.get(a, "LAND") is not None
        cache.put(b, "CELL", [os.urandom(100_000)], keys=["vault"])
        assert cache.get(a, "LAND") is None  # the largest tag's bodies went
        assert cache.get(b, "CELL") is not None  # the one just written was spared
        assert cache.holders([a], "LAND", "(0, 0)") == [a]  # its keys stayed
        assert cache.size_bytes() <= 300_000 + 65_536


def test_two_caches_on_one_file_do_not_lose_each_others_writes(tmp_path: Path) -> None:
    import threading

    db = tmp_path / "c.sqlite3"
    plugins = [_plugin(tmp_path, f"p{i}.esp") for i in range(40)]
    errors: list[BaseException] = []

    def work(cache: RecordCache, mine: list[Path]) -> None:
        try:
            for p in mine:
                cache.put_keys(p, "CELL", [p.stem])
                cache.holders(mine, "CELL", p.stem)
        except BaseException as exc:  # noqa: BLE001 - the point is that none escapes
            errors.append(exc)

    with RecordCache(db) as first, RecordCache(db) as second:
        threads = [
            threading.Thread(target=work, args=(first, plugins[0::2])),
            threading.Thread(target=work, args=(second, plugins[1::2])),
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=30)
    assert errors == []
    with RecordCache(db) as third:
        assert third.holders(plugins, "CELL", "p7") == [plugins[7]]
        assert third.missing_keys(plugins, "CELL") == []


def _use_world(tmp_path: Path) -> list[tuple[str, Path, list[dict[str, Any]]]]:
    sword = {"type": "Weapon", "id": "iron_sword", "data": {"weight": 1.0}}
    loot = {"type": "LeveledItem", "id": "l_loot", "items": [["iron_sword", 1]]}
    loot2 = {"type": "LeveledItem", "id": "l_loot", "items": [["other", 1]]}
    script = {"type": "Script", "id": "s_x", "text": "if ( player->GetItemCount iron_sword )"}
    cell = {
        "type": "Cell",
        "flags": "",
        "id": "",
        "name": "",
        "data": {"flags": "", "grid": [0, 0]},
        "references": [{"id": "iron_sword"}, {"id": "iron_sword"}, {"id": "other"}],
    }
    npc = {"type": "Npc", "id": "guard", "inventory": [[-1, "Iron_Sword"]]}
    head = {"type": "Header", "masters": []}
    return [
        ("a.esm", _plugin(tmp_path, "a.esm"), [head, sword, loot, script, cell]),
        ("b.esp", _plugin(tmp_path, "b.esp", b"y" * 12), [head, loot2, npc]),
    ]


def _use_session(world, cache, calls: dict[str, int] | None = None) -> EditorSession:
    recs = {p: r for _n, p, r in world}
    calls = calls if calls is not None else {"all": 0}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        return [r for r in recs[path] if r["type"] == {"WEAP": "Weapon"}.get(tag)]

    def read_all(path: Path) -> list[dict[str, Any]]:
        calls["all"] += 1
        return recs[path]

    return EditorSession(
        [(n, p) for n, p, _r in world], PatchQueue(), read=read, read_all=read_all, disk_cache=cache
    )


def test_the_use_index_answers_what_the_scan_answers(tmp_path: Path) -> None:
    world = _use_world(tmp_path)
    scan = _use_session(world, None)
    found = scan.find("WEAP", "iron_sword")
    assert found is not None
    want = scan.uses(found)
    assert {u["key"] for u in want["uses"]} == {"l_loot", "s_x", "(0, 0)", "guard"}
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        s = _use_session(world, cache)
        assert s.index_uses(workers=1) == 2
        calls = {"all": 0}
        s2 = _use_session(world, cache, calls)
        found2 = s2.find("WEAP", "iron_sword")
        assert found2 is not None
        got = s2.uses(found2)
        assert calls["all"] == 0  # from the index: no plugin read
    assert got == want  # the same uses, counts, paths and winners, in the same order
    loot = next(u for u in got["uses"] if u["key"] == "l_loot")
    assert loot["wins"] is False  # b.esp replaced the list, so a.esm's use is out


def test_a_use_report_that_had_to_scan_indexes_for_the_next_click(tmp_path: Path) -> None:
    world = _use_world(tmp_path)
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        calls = {"all": 0}
        s = _use_session(world, cache, calls)
        found = s.find("WEAP", "iron_sword")
        assert found is not None
        first = s.uses(found)  # not indexed: scans (and starts indexing)
        assert s._use_indexer is not None
        s._use_indexer.join(timeout=10)
        scanned = calls["all"]
        assert scanned >= 2
        assert s.uses(found) == first
        assert calls["all"] == scanned  # the second click read nothing


def test_a_plugin_that_changed_is_scanned_not_indexed(tmp_path: Path) -> None:
    world = _use_world(tmp_path)
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        s = _use_session(world, cache)
        s.index_uses(workers=1)
        world[1][1].write_bytes(b"z" * 40)  # b.esp is now another file
        assert cache.missing_uses([p for _n, p, _r in world]) == [world[1][1]]
        calls = {"all": 0}
        s2 = _use_session(world, cache, calls)
        found = s2.find("WEAP", "iron_sword")
        assert found is not None
        s2.uses(found)
        assert calls["all"] >= 2  # fell back to the scan, which reads every plugin


def test_an_id_the_index_does_not_take_is_scanned(tmp_path: Path) -> None:
    from wraithguard.patch.usesindex import indexable

    assert indexable("iron_sword") and indexable("Gold_001")
    assert not indexable("iron sword")  # a space: a script word rule the index cannot share
    assert not indexable("x" * 65)
    assert not indexable("")


def test_a_cache_over_its_limit_gives_up_the_use_index_last(tmp_path: Path) -> None:
    world = _use_world(tmp_path)
    with RecordCache(tmp_path / "c.sqlite3", max_bytes=1) as cache:
        s = _use_session(world, cache)
        s.index_uses(workers=1)
        assert cache.uses_for([p for _n, p, _r in world], "iron_sword") is None


def test_indexing_can_be_stopped_between_records(tmp_path: Path) -> None:
    world = _use_world(tmp_path)
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        s = _use_session(world, cache)
        s._stop.set()  # the session is being replaced
        assert s.index_uses(workers=1) == 0
        assert cache.missing_uses([p for _n, p, _r in world]) == [p for _n, p, _r in world]


def test_a_trim_that_frees_most_of_the_file_gives_the_disk_back(tmp_path: Path) -> None:
    import os

    plugin = _plugin(tmp_path, "a.esm")
    db = tmp_path / "c.sqlite3"
    with RecordCache(db, max_bytes=2_000_000) as cache:
        cache.put(plugin, "LAND", [{"type": "Landscape", "blob": os.urandom(6_000_000)}], ["x"])
        assert cache.get(plugin, "LAND") is None  # over the limit: its bodies went
    assert db.stat().st_size < 1_000_000  # and the file is not still holding their pages
