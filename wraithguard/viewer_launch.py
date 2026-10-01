"""Finding and starting ``wraithguard-viewer``, the Tauri viewer shell.

One place that knows, for every way the toolkit is shipped, where the viewer is and
what it needs around it to start:

* **Linux Flatpak** (``packaging/flatpak``, the Steam Deck's build): the viewer is in
  ``/app/bin`` on the GNOME runtime's WebKitGTK, and the launcher names it
  (``WRAITHGUARD_VIEWER``).
* **Linux, plain build** (build-linux.yml, PyInstaller one-file): the viewer is in the
  bundle as a data file, not a binary - a binary would make PyInstaller collect
  WebKitGTK's whole dependency tree with it - and runs on the machine's own
  WebKitGTK 4.1. A data file may come out without its executable bit, so it is put
  back before the viewer is started. On Linux the viewer starts without the DMA-BUF
  renderer, which fails on several GPU stacks, the Deck's included.
* **Windows, WebView2-bundled build**: the Fixed Version WebView2 runtime ships in a
  ``webview2`` folder beside the app, for machines without the system runtime (older
  Windows, or WebView2 removed along with Edge). ``WEBVIEW2_BROWSER_EXECUTABLE_FOLDER``
  points the viewer at it. With no folder, the system runtime is used as before.
* **Everywhere else**: beside the app (PyInstaller's ``_MEIPASS``), or a local cargo
  build in a checkout.
"""

from __future__ import annotations

import contextlib
import os
import stat
import sys
from pathlib import Path

from wraithguard.proc import host_library_path

VIEWER_NAME = "wraithguard-viewer.exe" if os.name == "nt" else "wraithguard-viewer"

#: The file that marks a WebView2 Fixed Version runtime folder.
_WEBVIEW2_EXE = "msedgewebview2.exe"


def _app_roots() -> list[Path]:
    """Folders the app's own files can be in, most specific first."""
    roots: list[Path] = []
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        roots.append(Path(meipass))
    if getattr(sys, "frozen", False):
        roots.append(Path(sys.executable).resolve().parent)
    here = Path(__file__).resolve().parents[1]
    roots += [here, here / "viewer-shell" / "target" / "release"]
    return roots


def viewer_binary() -> str | None:
    """The viewer executable, or None when this install has none.

    Returns:
        Its path.
    """
    forced = os.environ.get("WRAITHGUARD_VIEWER")
    if forced and Path(forced).is_file():
        return forced
    for root in _app_roots():
        candidate = root / VIEWER_NAME
        if candidate.is_file():
            _ensure_executable(candidate)
            return str(candidate)
    return None


def _ensure_executable(path: Path) -> None:
    """Give the viewer back its executable bit if the bundle dropped it.

    The Linux one-file build carries the viewer as data (see the module docstring),
    and PyInstaller may extract a data file without the bit. A copy that cannot be
    changed is left as it is; starting it then fails and the app falls back to the
    browser, as for any viewer that will not start.

    Args:
        path: The viewer executable.
    """
    if os.name == "nt" or os.access(path, os.X_OK):
        return
    with contextlib.suppress(OSError):  # a read-only install: nothing to do here
        path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def bundled_webview2() -> str | None:
    """The WebView2 Fixed Version runtime shipped with the app, if this build has one.

    Returns:
        The folder holding ``msedgewebview2.exe``, or None.
    """
    if os.name != "nt":
        return None
    for root in _app_roots():
        base = root / "webview2"
        if (base / _WEBVIEW2_EXE).is_file():
            return str(base)
        # As Microsoft ships it: one versioned folder inside.
        if base.is_dir():
            for sub in sorted(base.iterdir(), reverse=True):
                if (sub / _WEBVIEW2_EXE).is_file():
                    return str(sub)
    return None


def viewer_env() -> dict[str, str]:
    """The environment the viewer starts with.

    Returns:
        A copy of this process's environment with the viewer's own settings added
        (an explicit setting in the environment is always left as it is).
    """
    env = dict(os.environ)
    if sys.platform.startswith("linux"):
        # Not the toolkit's bundled libraries (an older libstdc++ among them): the
        # host GPU driver loaded into the viewer cannot start with them, and WebKit
        # aborts with EGL_BAD_PARAMETER. The viewer uses the system's libraries.
        # Start-up already did this for the whole process; again here in case not.
        host_library_path(env)
        env.setdefault("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
    runtime = bundled_webview2()
    if runtime:
        env.setdefault("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", runtime)
    return env


def viewer_popen_kwargs() -> dict[str, object]:
    """Everything ``subprocess.Popen`` needs to start the viewer, besides argv.

    Returns:
        ``env``, and on Windows ``creationflags`` = CREATE_NO_WINDOW -- not the
        SW_HIDE startup info, which the viewer's first window would inherit and so
        never show.
    """
    kw: dict[str, object] = {"env": viewer_env()}
    if os.name == "nt":
        kw["creationflags"] = 0x08000000
    return kw
