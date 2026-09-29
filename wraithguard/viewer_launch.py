"""Finding and starting ``wraithguard-viewer``, the Tauri viewer shell.

One place that knows, for every way the toolkit is shipped, where the viewer is and
what it needs around it to start:

* **Linux AppImage** (``packaging/linux``): the viewer is inside the AppImage with
  WebKitGTK and its whole dependency tree beside it, so it runs on a machine with no
  WebKitGTK at all (a Steam Deck). The AppImage's ``AppRun`` says where
  (``WRAITHGUARD_VIEWER``) and which folder it must start in
  (``WRAITHGUARD_VIEWER_CWD``: the bundled WebKit finds its helper processes relative
  to it). A bundled WebKit also runs without its bubblewrap sandbox (the viewer only
  ever shows its own local page) and without the DMA-BUF renderer, which fails on
  several GPU stacks, the Deck's included.
* **Windows, WebView2-bundled build**: the Fixed Version WebView2 runtime ships in a
  ``webview2`` folder beside the app, for machines without the system runtime (older
  Windows, or WebView2 removed along with Edge). ``WEBVIEW2_BROWSER_EXECUTABLE_FOLDER``
  points the viewer at it. With no folder, the system runtime is used as before.
* **Everywhere else**: beside the app (PyInstaller's ``_MEIPASS``), or a local cargo
  build in a checkout.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

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
            return str(candidate)
    return None


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
        env.setdefault("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
        if env.get("APPDIR") or env.get("WRAITHGUARD_VIEWER_CWD"):
            env.setdefault("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", "1")
    runtime = bundled_webview2()
    if runtime:
        env.setdefault("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", runtime)
    return env


def viewer_popen_kwargs() -> dict[str, object]:
    """Everything ``subprocess.Popen`` needs to start the viewer, besides argv.

    Returns:
        ``env``, ``cwd`` (the AppImage's), and on Windows ``creationflags`` =
        CREATE_NO_WINDOW -- not the SW_HIDE startup info, which the viewer's first
        window would inherit and so never show.
    """
    kw: dict[str, object] = {"env": viewer_env()}
    cwd = os.environ.get("WRAITHGUARD_VIEWER_CWD")
    if cwd and Path(cwd).is_dir():
        kw["cwd"] = cwd
    if os.name == "nt":
        kw["creationflags"] = 0x08000000
    return kw
