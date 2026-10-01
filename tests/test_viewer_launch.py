"""Where the viewer shell is, and what it is started with, per way of shipping."""

from __future__ import annotations

import os
import sys
from typing import TYPE_CHECKING

import pytest

from wraithguard import viewer_launch as vl

if TYPE_CHECKING:
    from pathlib import Path


def test_the_flatpak_names_its_own_viewer(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """``WRAITHGUARD_VIEWER`` (set by the Flatpak's launcher) wins over any other copy."""
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


def test_the_linux_webkit_settings(monkeypatch: pytest.MonkeyPatch) -> None:
    """On Linux DMA-BUF is off; WebKit's sandbox stays on and the viewer keeps our cwd."""
    monkeypatch.delenv("WEBKIT_DISABLE_DMABUF_RENDERER", raising=False)
    monkeypatch.delenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", raising=False)
    kw = vl.viewer_popen_kwargs()
    assert "cwd" not in kw
    if sys.platform.startswith("linux"):
        env = kw["env"]
        assert env["WEBKIT_DISABLE_DMABUF_RENDERER"] == "1"
        assert "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS" not in env


def test_an_explicit_setting_is_left_alone(monkeypatch: pytest.MonkeyPatch) -> None:
    """Someone who set the renderer themselves keeps their choice."""
    monkeypatch.setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "0")
    assert vl.viewer_env()["WEBKIT_DISABLE_DMABUF_RENDERER"] == "0"


@pytest.mark.parametrize(("orig", "expected"), [("/home/me/lib", "/home/me/lib"), (None, None)])
def test_the_frozen_apps_library_path_is_not_passed_on(
    monkeypatch: pytest.MonkeyPatch, orig: str | None, expected: str | None
) -> None:
    """PyInstaller's LD_LIBRARY_PATH (its own folder) is undone for the viewer.

    The toolkit's bundled libraries - an older libstdc++ among them - would otherwise
    load into the viewer ahead of the host's, and the Deck's GPU driver cannot start
    against them (EGL_BAD_PARAMETER). The value before PyInstaller set it comes back.
    """
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "_MEIPASS", "/opt/wraithguard/_internal", raising=False)
    monkeypatch.setenv("LD_LIBRARY_PATH", "/opt/wraithguard/_internal")
    if orig is None:
        monkeypatch.delenv("LD_LIBRARY_PATH_ORIG", raising=False)
    else:
        monkeypatch.setenv("LD_LIBRARY_PATH_ORIG", orig)
    env = vl.viewer_env()
    assert env.get("LD_LIBRARY_PATH") == expected
    assert "LD_LIBRARY_PATH_ORIG" not in env


def test_an_unfrozen_library_path_is_left_alone(monkeypatch: pytest.MonkeyPatch) -> None:
    """From a checkout there is no PyInstaller path to undo."""
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.delattr(sys, "frozen", raising=False)
    monkeypatch.setenv("LD_LIBRARY_PATH", "/home/me/lib")
    assert vl.viewer_env()["LD_LIBRARY_PATH"] == "/home/me/lib"


def test_only_the_bundle_folder_is_dropped(monkeypatch: pytest.MonkeyPatch) -> None:
    """With no saved original, the user's own entries survive; calling twice is harmless."""
    from wraithguard.proc import host_library_path

    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "_MEIPASS", "/opt/wraithguard/_internal", raising=False)
    env = {"LD_LIBRARY_PATH": os.pathsep.join(["/opt/wraithguard/_internal", "/home/me/lib"])}
    host_library_path(env)
    host_library_path(env)
    assert env == {"LD_LIBRARY_PATH": "/home/me/lib"}


@pytest.mark.skipif(os.name == "nt", reason="no executable bit on Windows")
def test_a_bundled_viewer_gets_its_executable_bit(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The one-file Linux build carries the viewer as data; it must still start."""
    exe = tmp_path / vl.VIEWER_NAME
    exe.write_bytes(b"")
    exe.chmod(0o644)
    monkeypatch.delenv("WRAITHGUARD_VIEWER", raising=False)
    monkeypatch.setattr(vl, "_app_roots", lambda: [tmp_path])
    assert vl.viewer_binary() == str(exe)
    assert os.access(exe, os.X_OK)


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
