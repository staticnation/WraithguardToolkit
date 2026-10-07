"""Test in OpenMW: the command line, finding OpenMW, and the ``editTestRun`` link."""

from __future__ import annotations

import json
import sys
import time
from pathlib import Path

import pytest

from tests.test_editorlink import _Builder, _post
from wraithguard.patch import testrun


def test_an_interior_starts_by_name():
    argv, script = testrun.launch_args(Path("/g/openmw"), Path("/t/Wraithguard Test.esp"), "Vault")
    assert argv[1:] == [
        "--data",
        str(Path("/t")),
        "--content",
        "Wraithguard Test.esp",
        "--skip-menu",
        "--start",
        "Vault",
    ]
    assert script is None


@pytest.mark.parametrize("cell", ["(-3, -2)", "-3,-2", "Balmora (-3, -2)"])
def test_an_exterior_goes_by_its_grid(cell):
    argv, script = testrun.launch_args(Path("openmw"), Path("t/p.esp"), cell)
    assert "--start" not in argv
    assert script == "coe -3 -2\n"


def test_no_cell_is_the_games_own_start():
    argv, script = testrun.launch_args(Path("openmw"), Path("t/p.esp"), None)
    assert "--start" not in argv and script is None


def test_openmw_given_as_its_folder_is_found_and_remembered(tmp_path, monkeypatch):
    monkeypatch.setattr(testrun, "_remembered_file", lambda: tmp_path / "openmw_test.json")
    exe = tmp_path / ("openmw.exe" if sys.platform == "win32" else "openmw")
    exe.write_bytes(b"")
    assert testrun.find_openmw(str(tmp_path)) == exe
    assert json.loads((tmp_path / "openmw_test.json").read_text(encoding="utf-8"))["exe"] == str(
        exe
    )
    assert testrun.find_openmw() == exe  # remembered
    assert testrun.find_openmw(str(tmp_path / "nope")) is None


def test_the_link_writes_the_plugin_keeps_the_pool_and_starts_the_game(tmp_path, monkeypatch):
    monkeypatch.setattr(testrun, "scratch_dir", lambda: tmp_path)
    monkeypatch.setattr(testrun, "_remembered_file", lambda: tmp_path / "openmw_test.json")
    host = _Builder(tmp_path)
    _post(host._on_edit_set, tag="NPC_", id="fargoth", path="data.level", value="7")
    # The "game" is this Python: it refuses OpenMW's arguments, prints why, and exits.
    job = _post(host._on_edit_test_run, cell="(1, 2)", exe=sys.executable)["job"]
    for _ in range(200):
        st = _post(host._on_edit_build_status, job=job, **{"from": 0})
        if st["state"] != "running":
            break
        time.sleep(0.05)
    assert st["state"] == "done", st
    assert (tmp_path / testrun.TEST_PLUGIN).read_bytes() == b"TES3"
    assert (tmp_path / "start.txt").read_text(encoding="utf-8") == "coe 1 2\n"
    assert any("--script-run" in line for line in st["lines"])
    assert st["result"]["exit"] != 0 and st["result"]["pid"]
    assert host._queue.fields, "the pool is kept"


def test_no_openmw_asks_where_it_is(tmp_path, monkeypatch):
    monkeypatch.setattr(testrun, "find_openmw", lambda given=None: None)
    host = _Builder(tmp_path)
    with pytest.raises(ValueError, match=r"^needExe: "):
        host._on_edit_test_run(b"{}")


def test_a_launch_setup_loads_first_and_runs_after_the_move():
    setup = testrun.clean_setup({"content": [" Helpers.esp ", ""], "script": "tgm"})
    argv, script = testrun.launch_args(Path("openmw"), Path("t/p.esp"), "(0, 0)", setup)
    i = argv.index("Helpers.esp")
    assert argv[i - 1] == "--content" and argv.index("p.esp") > i
    assert script == "coe 0 0\ntgm\n"
    _, only = testrun.launch_args(Path("openmw"), Path("t/p.esp"), "Vault", setup)
    assert only == "tgm\n"


def test_a_content_file_is_a_plugins_name():
    with pytest.raises(ValueError, match="plugin"):
        testrun.clean_setup({"content": ["../x.esp"]})
    with pytest.raises(ValueError, match="plugin"):
        testrun.clean_setup({"content": ["notes.txt"]})


def test_setups_are_kept_and_one_is_used(tmp_path, monkeypatch):
    monkeypatch.setattr(testrun, "_remembered_file", lambda: tmp_path / "openmw_test.json")
    host = _Builder(tmp_path)
    assert _post(host._on_edit_test_setups) == {"setups": {}, "use": ""}
    got = _post(
        host._on_edit_test_setups,
        setups={"Mine": {"content": ["A.esp"], "script": "tgm"}},
        use="Mine",
    )
    assert got == {"setups": {"Mine": {"content": ["A.esp"], "script": "tgm"}}, "use": "Mine"}
    testrun.remember_exe(Path(sys.executable))  # the setups survive the exe being kept
    assert testrun.load_settings()["use"] == "Mine"
    # Gone from the setups, it is no longer used.
    assert _post(host._on_edit_test_setups, setups={})["use"] == ""
    with pytest.raises(ValueError, match="No launch setup"):
        host._on_edit_test_run(json.dumps({"setup": "Nope", "exe": sys.executable}).encode())
