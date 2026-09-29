"""Where the viewer shell is, and what it is started with, per way of shipping."""

from __future__ import annotations

import os
import sys
from typing import TYPE_CHECKING

import pytest

from wraithguard import viewer_launch as vl

if TYPE_CHECKING:
    from pathlib import Path


def test_the_appimage_names_its_own_viewer(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """``WRAITHGUARD_VIEWER`` (set by the AppImage's AppRun) wins over any other copy."""
    exe = tmp_path / vl.VIEWER_NAME
    exe.write_bytes(b"")
    monkeypatch.setenv("WRAITHGUARD_VIEWER", str(exe))
    assert vl.viewer_binary() == str(exe)


def test_a_missing_forced_viewer_falls_through(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A stale override does not stop the other places being looked in."""
    monkeypatch.setenv("WRAITHGUARD_VIEWER", str(tmp_path / "gone"))
    monkeypatch.setattr(vl, "_app_roots", lambda: [tmp_path])
    assert vl.viewer_binary() is None


def test_the_appimage_cwd_and_webkit_settings(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """In the AppImage the viewer starts in its folder, sandbox and DMA-BUF off."""
    monkeypatch.setenv("WRAITHGUARD_VIEWER_CWD", str(tmp_path))
    monkeypatch.setenv("APPDIR", str(tmp_path))
    monkeypatch.delenv("WEBKIT_DISABLE_DMABUF_RENDERER", raising=False)
    kw = vl.viewer_popen_kwargs()
    assert kw["cwd"] == str(tmp_path)
    if sys.platform.startswith("linux"):
        env = kw["env"]
        assert env["WEBKIT_DISABLE_DMABUF_RENDERER"] == "1"
        assert env["WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS"] == "1"


def test_an_explicit_setting_is_left_alone(monkeypatch: pytest.MonkeyPatch) -> None:
    """Someone who set the renderer themselves keeps their choice."""
    monkeypatch.setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "0")
    assert vl.viewer_env()["WEBKIT_DISABLE_DMABUF_RENDERER"] == "0"


@pytest.mark.skipif(os.name != "nt", reason="WebView2 is Windows-only")
def test_the_bundled_webview2_is_found(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """The Fixed Version runtime's versioned folder, as Microsoft ships it."""
    runtime = tmp_path / "webview2" / "Microsoft.WebView2.FixedVersionRuntime.109.0.1518.140.x64"
    runtime.mkdir(parents=True)
    (runtime / "msedgewebview2.exe").write_bytes(b"")
    monkeypatch.setattr(vl, "_app_roots", lambda: [tmp_path])
    monkeypatch.delenv("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", raising=False)
    assert vl.bundled_webview2() == str(runtime)
    assert vl.viewer_env()["WEBVIEW2_BROWSER_EXECUTABLE_FOLDER"] == str(runtime)


def test_no_webview2_off_windows() -> None:
    """Nothing to find anywhere but Windows."""
    if os.name != "nt":
        assert vl.bundled_webview2() is None
