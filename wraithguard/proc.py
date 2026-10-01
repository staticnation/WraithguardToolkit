"""Subprocess helpers shared across the toolkit.

:func:`no_window_kwargs` stops a windowed (PyInstaller ``--noconsole`` /
auto-py-to-exe) build from flashing a console window every time it shells out to a
console program like ``tes3conv``. It lives in the package -- rather than in the
top-level script -- so the merge and patch services can use it without importing
the script back, which would be a layering inversion. On any non-Windows platform
it is a no-op.

:func:`restore_host_library_path` undoes, for every child process, the library
path a frozen Linux build runs with (see its docstring).
"""

from __future__ import annotations

import os
import subprocess
import sys
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import MutableMapping

_LD = "LD_LIBRARY_PATH"
_LD_ORIG = "LD_LIBRARY_PATH_ORIG"


def host_library_path(env: MutableMapping[str, str]) -> None:
    """Take a frozen Linux build's own library folder out of ``env``'s search path.

    PyInstaller's bootloader puts the app's folder (``sys._MEIPASS``) on
    ``LD_LIBRARY_PATH`` and keeps any earlier value in ``LD_LIBRARY_PATH_ORIG``. The
    app itself needs that, but its children do not: a host program started with it
    loads the build machine's libraries instead of its own. On a Steam Deck that
    broke ``/bin/sh`` (bash wanting a newer libreadline: "undefined symbol:
    rl_trim_arg_from_keyseq") and the viewer's GPU driver (an older libstdc++:
    EGL_BAD_PARAMETER). The value from before PyInstaller set it comes back.

    Safe to call more than once. Does nothing outside a frozen Linux build.

    Args:
        env: An environment mapping, changed in place.
    """
    if not sys.platform.startswith("linux") or not getattr(sys, "frozen", False):
        return
    if _LD_ORIG in env:
        env[_LD] = env.pop(_LD_ORIG)
        return
    bundle = getattr(sys, "_MEIPASS", None)
    if not bundle or _LD not in env:
        return
    own = os.path.normpath(bundle)
    kept = [p for p in env[_LD].split(os.pathsep) if p and os.path.normpath(p) != own]
    if kept:
        env[_LD] = os.pathsep.join(kept)
    else:
        del env[_LD]


def restore_host_library_path() -> None:
    """Apply :func:`host_library_path` to this process's environment.

    Called once at start-up, so every child process -- the viewer, ``xdg-open``,
    tes3cmd, a shell -- starts with the host's libraries. The running app is not
    affected: the dynamic loader read ``LD_LIBRARY_PATH`` when it started.
    """
    host_library_path(os.environ)


def no_window_kwargs() -> dict[str, Any]:
    """Return ``subprocess`` kwargs that suppress the Windows console flash.

    A ``--noconsole`` build has no console of its own, so each child console
    process it launches opens one instead -- a popup per plugin during a Merged
    Lands run, which is what this prevents. ``CREATE_NO_WINDOW`` stops the
    window; the hidden ``STARTUPINFO`` is a belt-and-braces for shells that
    honour ``SW_HIDE`` rather than the creation flag. Redirecting the child's
    stdout/stderr (``capture_output``/``DEVNULL``) does **not** do this on its
    own: it hides the *output*, not the *window*.

    Returns:
        Keyword arguments to splat into ``subprocess.run`` or ``Popen``. Empty
        on non-Windows platforms, where there is no console window to suppress.
    """
    if os.name != "nt":
        return {}
    kw: dict[str, Any] = {"creationflags": 0x08000000}  # CREATE_NO_WINDOW
    try:
        # Windows-only API; the whole block is guarded by os.name == "nt". The
        # ignore is needed when mypy checks as Linux (CI) and unused when it checks
        # as Windows, hence both codes.
        si = subprocess.STARTUPINFO()  # type: ignore[attr-defined,unused-ignore]
        si.dwFlags |= subprocess.STARTF_USESHOWWINDOW  # type: ignore[attr-defined,unused-ignore]
        si.wShowWindow = 0  # SW_HIDE
        kw["startupinfo"] = si
    except AttributeError:
        # A build without STARTUPINFO/STARTF_USESHOWWINDOW still gets
        # CREATE_NO_WINDOW, which is the part that matters; skip the refinement.
        pass
    return kw
