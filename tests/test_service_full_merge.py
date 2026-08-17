"""A genuine end-to-end Merged Lands run, when a real tes3conv is available.

Builds a master and two mods that all edit one cell's heights, converts them to
real ``.esp`` files with tes3conv, and merges them -- driving the record-writing,
texture-finishing, CELL-emitting and file-writing tail of ``build_merged_lands``
that a terrain-free run never reaches. Skips where no tes3conv is installed
(``find_tes3conv``: set ``MLOX_TES3CONV`` or put one on PATH).
"""

from __future__ import annotations

import json
import subprocess
from typing import TYPE_CHECKING

import pytest

import wraithguard_toolkit as core
from wraithguard.land.emit import build_landscape_record, build_plugin
from wraithguard.land.service import build_merged_lands
from wraithguard.tes3fields.landscape import LAND_SIZE

if TYPE_CHECKING:
    from pathlib import Path


def _grid(value: float) -> list[list[float]]:
    """A full 65x65 cell of one height."""
    return [[float(value)] * LAND_SIZE for _ in range(LAND_SIZE)]


def _write_plugin(
    conv: str,
    tmp: Path,
    name: str,
    records: list[dict],
    masters: list[tuple[str, int]],
) -> Path:
    """Build a plugin document, convert it to a real .esp via tes3conv."""
    document = build_plugin(records, masters)
    json_path = tmp / f"{name}.json"
    json_path.write_text(json.dumps(document), encoding="utf-8")
    esp = tmp / name
    subprocess.run(  # noqa: S603 -- fixed argv, a test-built tes3conv command
        [conv, str(json_path), str(esp), "--overwrite"],
        check=True,
        capture_output=True,
        text=True,
    )
    return esp


class TestFullMergeWritesRecords:
    """The tail of build_merged_lands: merge, finish, write, mark."""

    def test_a_two_mod_merge_writes_and_marks_the_output(self, tmp_path: Path) -> None:
        """Two mods contesting one cell produce a written, marked Merged Lands.esp."""
        conv = core.find_tes3conv()
        if not conv:
            pytest.skip("needs a real tes3conv (set MLOX_TES3CONV or put one on PATH)")

        master_rec, _ = build_landscape_record((0, 0), heights=_grid(100))
        master = _write_plugin(conv, tmp_path, "Master.esm", [master_rec], [("Base.esm", 1)])
        msize = master.stat().st_size

        a_rec, _ = build_landscape_record((0, 0), heights=_grid(200))
        _write_plugin(conv, tmp_path, "ModA.esp", [a_rec], [("Master.esm", msize)])
        b_rec, _ = build_landscape_record((0, 0), heights=_grid(300))
        _write_plugin(conv, tmp_path, "ModB.esp", [b_rec], [("Master.esm", msize)])

        out = tmp_path / "Merged Lands.esp"
        collected: list[str] = []
        result = build_merged_lands(
            [tmp_path],
            ["Master.esm", "ModA.esp", "ModB.esp"],
            converter=conv,
            output=out,
            include_cells=True,
            report=collected.append,
        )
        assert result.output == out
        assert out.is_file()
        assert result.cells_written >= 1
        assert any("wrote" in line for line in collected)
        # the generated-marker sidecar is written beside the output
        assert (tmp_path / "Merged Lands.mergedlands.toml").is_file()

    def test_a_dry_run_writes_nothing(self, tmp_path: Path) -> None:
        """dry_run merges and reports but leaves no file behind."""
        conv = core.find_tes3conv()
        if not conv:
            pytest.skip("needs a real tes3conv")

        master_rec, _ = build_landscape_record((0, 0), heights=_grid(100))
        master = _write_plugin(conv, tmp_path, "Master.esm", [master_rec], [("Base.esm", 1)])
        msize = master.stat().st_size
        a_rec, _ = build_landscape_record((0, 0), heights=_grid(200))
        _write_plugin(conv, tmp_path, "ModA.esp", [a_rec], [("Master.esm", msize)])
        b_rec, _ = build_landscape_record((0, 0), heights=_grid(300))
        _write_plugin(conv, tmp_path, "ModB.esp", [b_rec], [("Master.esm", msize)])

        out = tmp_path / "Merged Lands.esp"
        result = build_merged_lands(
            [tmp_path],
            ["Master.esm", "ModA.esp", "ModB.esp"],
            converter=conv,
            output=out,
            dry_run=True,
        )
        assert result.output is None
        assert not out.exists()
