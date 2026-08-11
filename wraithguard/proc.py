"""Subprocess helpers shared across the toolkit.

The one thing here is :func:`no_window_kwargs`, which stops a windowed
(PyInstaller ``--noconsole`` / auto-py-to-exe) build from flashing a console
window every time it shells out to a console program like ``tes3conv``. It lives
in the package -- rather than in the top-level script -- so the merge and patch
services can use it without importing the script back, which would be a layering
inversion. On any non-Windows platform it is a no-op.
"""

from __future__ import annotations

import os
import subprocess
from typing import Any


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
        # Windows-only API; the whole block is guarded by os.name == "nt".
        si = subprocess.STARTUPINFO()  # type: ignore[attr-defined]
        si.dwFlags |= subprocess.STARTF_USESHOWWINDOW  # type: ignore[attr-defined]
        si.wShowWindow = 0  # SW_HIDE
        kw["startupinfo"] = si
    except AttributeError:
        # A build without STARTUPINFO/STARTF_USESHOWWINDOW still gets
        # CREATE_NO_WINDOW, which is the part that matters; skip the refinement.
        pass
    return kw
