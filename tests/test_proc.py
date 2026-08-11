"""Tests for :func:`wraithguard.proc.no_window_kwargs`.

A ``--noconsole`` build flashes a console window per child process unless the
subprocess call passes ``CREATE_NO_WINDOW``. This is the one thing that stopped
a Merged Lands run from popping a window per plugin, so it is pinned here and
its use by the merge/patch encoders is checked in test_land_service.py.
"""

from __future__ import annotations

from wraithguard import proc
from wraithguard.proc import no_window_kwargs


class TestNoWindowKwargs:
    def test_non_windows_is_a_no_op(self, monkeypatch) -> None:
        """Off Windows there is no console window, so nothing is added."""
        monkeypatch.setattr(proc.os, "name", "posix")
        assert no_window_kwargs() == {}

    def test_windows_sets_create_no_window(self, monkeypatch) -> None:
        """CREATE_NO_WINDOW is the flag that suppresses the flash."""
        monkeypatch.setattr(proc.os, "name", "nt")
        kwargs = no_window_kwargs()
        assert kwargs["creationflags"] == 0x08000000

    def test_it_is_splattable_into_subprocess(self, monkeypatch) -> None:
        """The result must be a plain kwargs dict a caller can ``**`` in."""
        monkeypatch.setattr(proc.os, "name", "nt")
        kwargs = no_window_kwargs()
        assert isinstance(kwargs, dict)
        # STARTUPINFO is unavailable off Windows, so only creationflags is
        # guaranteed here -- which is the part that suppresses the window.
        assert set(kwargs) <= {"creationflags", "startupinfo"}
