"""The native session keeps each plugin's keys and cells between runs, in the record cache."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

import wraithguard_toolkit as core
from wraithguard.patch.recordcache import RecordCache

if TYPE_CHECKING:
    from pathlib import Path

KEYS = [("Weapon", "iron_sword", False), ("Cell", "(0, 0)", False)]
CELLS = [("ext", 0, 0)]


@pytest.fixture
def reads(monkeypatch: pytest.MonkeyPatch) -> list[str]:
    seen: list[str] = []

    def native_keys(path: Any) -> tuple[list[Any], list[Any]]:
        seen.append(str(path))
        return list(KEYS), list(CELLS)

    monkeypatch.setattr(core.NativeEspSession, "_native_keys", staticmethod(native_keys))
    return seen


def _session(tmp_path: Path, cache: RecordCache | None) -> core.NativeEspSession:
    return core.NativeEspSession(dump_dir=str(tmp_path / "dump"), disk_cache=cache)


def test_a_second_session_reads_no_plugin_the_first_read(tmp_path: Path, reads: list[str]) -> None:
    plugin = tmp_path / "a.esm"
    plugin.write_bytes(b"x" * 10)
    db = tmp_path / "c.sqlite3"
    for _run in range(2):
        with RecordCache(db) as cache:
            s = _session(tmp_path, cache)
            assert s.record_keys(plugin) == KEYS
            assert s.cells(plugin) == CELLS
            s.cleanup()
    assert reads == [str(plugin)]  # once, not once per run


def test_a_plugin_that_changed_is_read_again(tmp_path: Path, reads: list[str]) -> None:
    plugin = tmp_path / "a.esm"
    plugin.write_bytes(b"x" * 10)
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        _session(tmp_path, cache).record_keys(plugin)
        plugin.write_bytes(b"x" * 11)
        _session(tmp_path, cache).record_keys(plugin)
    assert len(reads) == 2


def test_without_a_cache_it_is_as_before(tmp_path: Path, reads: list[str]) -> None:
    plugin = tmp_path / "a.esm"
    plugin.write_bytes(b"x")
    s = _session(tmp_path, None)
    assert s.record_keys(plugin) == KEYS
    assert s.record_keys(plugin) == KEYS  # from memory
    assert len(reads) == 1


def test_an_unreadable_plugin_is_not_kept(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    plugin = tmp_path / "bad.esp"
    plugin.write_bytes(b"x")
    monkeypatch.setattr(core.NativeEspSession, "_native_keys", staticmethod(lambda _p: ([], [])))
    with RecordCache(tmp_path / "c.sqlite3") as cache:
        _session(tmp_path, cache).record_keys(plugin)
        assert cache.get(plugin, "~KC") is None  # a failed read is tried again, not remembered
