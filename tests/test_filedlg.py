"""The file-dialog wrapper's command lines (no dialog is opened)."""

from __future__ import annotations

import pytest

pytest.importorskip("tkinter")

from wraithguard.gui import filedlg


def test_plain_globs_undo_case_insensitive_patterns() -> None:
    """``*.[eE][sS][pP]`` becomes ``*.esp`` and ``*.ESP``; ``*`` stays one entry."""
    assert filedlg._plain_globs("*.[eE][sS][pP] *.[oO][mM][wW]addon") == [
        "*.esp",
        "*.ESP",
        "*.omwaddon",
        "*.OMWADDON",
    ]
    assert filedlg._plain_globs("*") == ["*"]


def test_zenity_open_many() -> None:
    """zenity: multiple selection, one path a line, filters and title."""
    cmd = filedlg._command(
        "zenity",
        "opens",
        {"title": "Plugins", "initialdir": "/nonexistent", "filetypes": [("Plugins", "*.esp")]},
    )
    assert cmd[:2] == ["zenity", "--file-selection"]
    assert "--multiple" in cmd
    assert "--separator=\n" in cmd
    assert "--file-filter=Plugins | *.esp *.ESP" in cmd
    assert cmd[-1] == "--title=Plugins"


def test_kdialog_directory_has_no_filter() -> None:
    """kdialog: a folder dialog takes no filter argument."""
    cmd = filedlg._command("kdialog", "dir", {"filetypes": [("Plugins", "*.esp")]})
    assert cmd == ["kdialog", "--getexistingdirectory", cmd[2]]


def test_kdialog_save_filter() -> None:
    """kdialog: filters as ``globs|label`` lines in one argument."""
    cmd = filedlg._command("kdialog", "save", {"filetypes": [("TOML", "*.toml"), ("All", "*")]})
    assert cmd[1] == "--getsavefilename"
    assert cmd[3] == "*.toml *.TOML|TOML\n*|All"


def test_tk_forced(monkeypatch: pytest.MonkeyPatch) -> None:
    """WRAITHGUARD_FILE_DIALOG=tk always uses Tk's own dialog."""
    monkeypatch.setenv("WRAITHGUARD_FILE_DIALOG", "tk")
    assert filedlg._native_tool() is None
