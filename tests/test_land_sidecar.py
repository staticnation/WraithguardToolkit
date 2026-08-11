"""The landscape sidecar: a compact per-plugin cache of just terrain records.

``Tes3ConvSession`` writes a ``<stem>.land.json`` beside every plugin it reads,
holding only the ``Landscape`` and ``LandscapeTexture`` records -- everything a
merge needs, a fraction of a plugin that may run to hundreds of MB. Merged Lands
reads it (via :func:`wraithguard.land.native.landscape_records_from_sidecar`)
instead of re-running the converter. These pin both halves: the session builds
and serves the sidecar, and the merge-side reader honours its version and
staleness rules.

The session works on already-dumped JSON, so no tes3conv binary is needed.
"""

from __future__ import annotations

import json
import os
from typing import TYPE_CHECKING, Any

import wraithguard_toolkit as core
from wraithguard.land.native import landscape_records_from_sidecar

if TYPE_CHECKING:
    from pathlib import Path

#: One of every record kind, so "only the landscape records" is a real filter.
RECORDS: list[dict[str, Any]] = [
    {"type": "TES3", "author": "header"},
    {"type": "Npc", "id": "bob", "level": 3},
    {"type": "Cell", "data": {"flags": 0, "grid": [5, 6]}, "name": ""},
    {"type": "Landscape", "grid": [5, 6], "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS"},
    {"type": "LandscapeTexture", "id": "my_tex", "index": 0, "file_name": "tx.dds"},
    {"type": "Dialogue", "id": "chatter"},
]


def _session(tmp_path: Path) -> tuple[core.Tes3ConvSession, str]:
    """A session whose dump dir already holds RECORDS as one plugin's JSON."""
    session = core.Tes3ConvSession(exe="unused", dump_dir=str(tmp_path), keep=True)
    (tmp_path / "Plugin.json").write_text(json.dumps(RECORDS), encoding="utf-8")
    return session, "Plugin.esp"


class TestSessionBuildsTheSidecar:
    def test_it_returns_only_landscape_records(self, tmp_path: Path) -> None:
        """The NPC, cell, header and dialogue are dropped; terrain is kept."""
        session, path = _session(tmp_path)
        land = session.landscape_records(path)
        assert [rec["type"] for rec in land] == ["Landscape", "LandscapeTexture"]

    def test_it_writes_a_reusable_sidecar_file(self, tmp_path: Path) -> None:
        """The second read is served from the file, not a fresh full parse."""
        session, path = _session(tmp_path)
        session.landscape_records(path)
        side = tmp_path / "Plugin.land.json"
        assert side.is_file()
        document = json.loads(side.read_text(encoding="utf-8"))
        assert document["v"] == core.Tes3ConvSession._SIDECAR_VER
        assert [rec["type"] for rec in document["d"]] == ["Landscape", "LandscapeTexture"]

    def test_a_plugin_with_no_terrain_caches_an_empty_list(self, tmp_path: Path) -> None:
        """No terrain is an answer, not a miss -- and it should still cache."""
        session = core.Tes3ConvSession(exe="unused", dump_dir=str(tmp_path), keep=True)
        (tmp_path / "Bare.json").write_text(
            json.dumps([{"type": "Npc", "id": "bob"}]), encoding="utf-8"
        )
        assert session.landscape_records("Bare.esp") == []
        assert (tmp_path / "Bare.land.json").is_file()


class TestParallelPrime:
    """prime() converts many plugins at once, warming the on-disk cache."""

    def test_it_reports_and_caches_every_conversion(self, tmp_path: Path) -> None:
        """Each primed plugin ends with a usable JSON the session then serves.

        No tes3conv binary is needed: a JSON already in the dump dir reads back
        as a converted plugin would, so prime just registers each as cached.
        """
        session = core.Tes3ConvSession(exe="unused", dump_dir=str(tmp_path), keep=True)
        names = []
        for i in range(6):
            (tmp_path / f"P{i}.json").write_text(
                json.dumps([{"type": "Npc", "id": str(i)}]), encoding="utf-8"
            )
            names.append(f"P{i}.esp")

        primed = session.prime(names, max_workers=3)

        assert primed == 6
        assert all(session._json_for(name) for name in names)

    def test_an_empty_list_is_a_no_op(self, tmp_path: Path) -> None:
        """Priming nothing converts nothing rather than erroring."""
        session = core.Tes3ConvSession(exe="unused", dump_dir=str(tmp_path), keep=True)
        assert session.prime([]) == 0

    def test_duplicates_are_converted_once(self, tmp_path: Path) -> None:
        """The same plugin listed twice is one conversion, not two."""
        session = core.Tes3ConvSession(exe="unused", dump_dir=str(tmp_path), keep=True)
        (tmp_path / "Dup.json").write_text(json.dumps([]), encoding="utf-8")
        assert session.prime(["Dup.esp", "Dup.esp"]) == 1


class TestTheMergeSideReader:
    def _write_plugin_and_sidecar(self, tmp_path: Path) -> Path:
        """A plugin file plus a fresh sidecar the reader should accept."""
        session, name = _session(tmp_path)
        session.landscape_records(name)  # writes Plugin.land.json
        plugin = tmp_path / "Plugin.esp"
        plugin.write_bytes(b"TES3 not really, just for the mtime")
        # The sidecar must be at least as new as the plugin to be trusted.
        side = tmp_path / "Plugin.land.json"
        now = side.stat().st_mtime
        os.utime(plugin, (now - 10, now - 10))
        return plugin

    def test_a_fresh_sidecar_is_used(self, tmp_path: Path) -> None:
        """A current sidecar returns the terrain the merge would have converted."""
        plugin = self._write_plugin_and_sidecar(tmp_path)
        records = landscape_records_from_sidecar(plugin, tmp_path)
        assert records is not None
        assert [rec["type"] for rec in records] == ["Landscape", "LandscapeTexture"]

    def test_a_stale_sidecar_is_refused(self, tmp_path: Path) -> None:
        """A plugin edited after its sidecar must be re-read, not trusted."""
        plugin = self._write_plugin_and_sidecar(tmp_path)
        side = tmp_path / "Plugin.land.json"
        old = side.stat().st_mtime
        os.utime(plugin, (old + 100, old + 100))  # plugin now newer than sidecar
        assert landscape_records_from_sidecar(plugin, tmp_path) is None

    def test_an_absent_sidecar_is_no_answer(self, tmp_path: Path) -> None:
        """No sidecar means the caller must convert the plugin itself."""
        plugin = tmp_path / "Missing.esp"
        plugin.write_bytes(b"x")
        assert landscape_records_from_sidecar(plugin, tmp_path) is None

    def test_a_wrong_version_sidecar_is_refused(self, tmp_path: Path) -> None:
        """A sidecar from a different schema is ignored rather than guessed at."""
        plugin = tmp_path / "Plugin.esp"
        plugin.write_bytes(b"x")
        side = tmp_path / "Plugin.land.json"
        side.write_text(json.dumps({"v": -1, "d": [{"type": "Landscape"}]}), encoding="utf-8")
        now = side.stat().st_mtime
        os.utime(plugin, (now - 10, now - 10))
        assert landscape_records_from_sidecar(plugin, tmp_path) is None
