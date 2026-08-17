"""``no_window_kwargs``'s three real outcomes: not Windows, Windows with
STARTUPINFO support, and Windows without it.

That last one is the gap worth naming: on a real Windows machine
``subprocess.STARTUPINFO`` always succeeds, so a test suite run there never
exercises the ``except AttributeError`` fallback -- it only exists for a
build whose ``subprocess`` lacks that class/attribute entirely. Deleting the
attribute is how this gets covered without needing that build.
"""

from __future__ import annotations

import os
import subprocess

import pytest

from wraithguard.proc import no_window_kwargs


class TestNoWindowKwargs:
    def test_non_windows_returns_an_empty_dict(self, monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(os, "name", "posix")
        assert no_window_kwargs() == {}

    def test_windows_with_startupinfo_support_sets_both(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(os, "name", "nt")
        if not hasattr(subprocess, "STARTUPINFO"):
            pytest.skip("subprocess.STARTUPINFO is not available on this platform")

        kw = no_window_kwargs()

        assert kw["creationflags"] == 0x08000000
        info = kw["startupinfo"]
        assert info.dwFlags & subprocess.STARTF_USESHOWWINDOW
        assert info.wShowWindow == 0

    def test_windows_without_startupinfo_still_suppresses_the_window(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """The refinement (hiding via STARTUPINFO) is optional; the flag that
        actually stops the console flash (CREATE_NO_WINDOW) is not, and must
        survive even when STARTUPINFO isn't there to set."""
        monkeypatch.setattr(os, "name", "nt")
        monkeypatch.delattr(subprocess, "STARTUPINFO", raising=False)

        kw = no_window_kwargs()

        assert kw == {"creationflags": 0x08000000}
        assert "startupinfo" not in kw
