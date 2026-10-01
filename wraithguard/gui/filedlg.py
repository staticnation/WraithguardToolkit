"""File dialogs: the desktop's own on Linux, Tk's as the fallback.

A drop-in for :mod:`tkinter.filedialog` (``from wraithguard.gui import filedlg as
filedialog``): the same four functions, the same keyword arguments, the same results
(``""`` or ``()`` on cancel).

Why: on Windows and macOS Tk already uses the system's dialog. On Linux it draws its own,
which (a) ignores the app's dark theme - a white list in a dark window - and (b) hides
dot-folders with no way to show them, so ``~/.config/openmw/openmw.cfg`` could not be
picked at all. So on Linux this asks the desktop's own dialog first - ``kdialog`` on
KDE (the Steam Deck's desktop), ``zenity`` elsewhere - which follows the system theme
and shows hidden files on Ctrl+H (KDE: Alt+.). Without either (inside the Flatpak,
say) it is Tk's dialog, with hidden files shown and its "Show hidden files" toggle on.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import time
import tkinter as tk
from pathlib import Path
from tkinter import filedialog as _tk
from typing import TYPE_CHECKING, Any

from wraithguard.logging_setup import get_logger

if TYPE_CHECKING:
    from collections.abc import Sequence

LOG = get_logger(__name__)

#: Set WRAITHGUARD_FILE_DIALOG=tk to always use Tk's dialog (or =kdialog / =zenity).
_ENV = "WRAITHGUARD_FILE_DIALOG"

#: The desktop dialog open now, if any (the app keeps drawing while it is open).
_open: list[subprocess.Popen[str]] = []


def _linux() -> bool:
    """True on Linux, where Tk draws its own file dialog."""
    return sys.platform.startswith("linux")


def _native_tool() -> str | None:
    """The desktop's dialog program to use, or None for Tk's own dialog.

    Returns:
        ``"kdialog"``, ``"zenity"`` or None.
    """
    if not _linux():
        return None
    forced = os.environ.get(_ENV, "").strip().lower()
    if forced == "tk":
        return None
    if forced in ("kdialog", "zenity"):
        return forced if shutil.which(forced) else None
    kde = "KDE" in os.environ.get("XDG_CURRENT_DESKTOP", "").upper()
    order = ("kdialog", "zenity") if kde else ("zenity", "kdialog")
    for tool in order:
        if shutil.which(tool):
            return tool
    return None


# ---------------------------------------------------------------- Tk's own dialog --


def _prepare_tk(parent: tk.Misc | None) -> None:
    """Tk's Linux dialog: hidden files shown, and its toggle for them on.

    The dialog's code loads on first use; asking for it with a bad option loads it
    (the usual Tk idiom) so its settings exist to be set.

    Args:
        parent: Any widget of the app, for the Tcl interpreter.
    """
    if not _linux():
        return
    try:
        root = parent or tk._get_default_root()  # type: ignore[attr-defined,unused-ignore]
        root.tk.call("catch", "tk_getOpenFile -wraithguard-load")
        root.tk.call("set", "::tk::dialog::file::showHiddenBtn", "1")
        root.tk.call("set", "::tk::dialog::file::showHiddenVar", "1")
    except (tk.TclError, RuntimeError, AttributeError):
        LOG.debug("could not set Tk's hidden-file options", exc_info=True)


# ------------------------------------------------------------ kdialog and zenity --


def _plain_globs(patterns: str) -> list[str]:
    """Tk filter globs as plain ones, in both cases.

    ``case_insensitive_filetypes`` writes ``*.[eE][sS][pP]`` for Tk; the desktops'
    dialogs want ``*.esp`` (and match case-sensitively, hence ``*.ESP`` beside it).

    Args:
        patterns: A space-separated glob string.

    Returns:
        The globs, plain, each in lower and upper case.
    """
    out: list[str] = []
    for raw in patterns.split():
        g = re.sub(r"\[([A-Za-z])([A-Za-z])\]", lambda m: m.group(1).lower(), raw)
        for v in (g, g.upper()):
            if v not in out:
                out.append(v)
    return out


def _filters(tool: str, filetypes: Sequence[tuple[str, str]] | None) -> list[str]:
    """``filetypes`` as the tool's own filter arguments.

    Args:
        tool: ``"kdialog"`` or ``"zenity"``.
        filetypes: Tk's ``(label, patterns)`` pairs.

    Returns:
        The arguments to add (zenity), or a one-element list holding kdialog's filter.
    """
    pairs = [(label, _plain_globs(p)) for label, p in (filetypes or ())]
    if not pairs:
        return []
    if tool == "zenity":
        return [f"--file-filter={label} | {' '.join(g)}" for label, g in pairs]
    return ["\n".join(f"{' '.join(g)}|{label}" for label, g in pairs)]


def _start(initialdir: str | None, initialfile: str | None) -> str:
    """Where the dialog opens: the file in the folder, the folder, or home."""
    d = Path(initialdir).expanduser() if initialdir else Path.home()
    if not d.is_dir():
        d = Path.home()
    return str(d / initialfile) if initialfile else str(d) + os.sep


def _run(cmd: list[str], parent: tk.Misc | None) -> str | None:
    """Run a dialog program, keeping the app's window drawn while it is open.

    Args:
        cmd: The command line.
        parent: A widget to keep updating (its window redraws instead of freezing).

    Returns:
        What the user chose (stdout, stripped), "" when cancelled, or None when the
        program could not run - the caller then falls back to Tk's dialog.
    """
    if _open:
        return ""  # one at a time: a second click while one is open does nothing
    try:
        proc = subprocess.Popen(  # noqa: S603 - a fixed program name and our own arguments
            cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True
        )
    except OSError:
        LOG.debug("could not start %s", cmd[0], exc_info=True)
        return None
    widget = parent or getattr(tk, "_default_root", None)
    _open.append(proc)
    try:
        while proc.poll() is None:
            try:
                if widget is not None:
                    widget.update()
            except tk.TclError:
                pass  # the window was closed meanwhile; keep waiting for the dialog
            time.sleep(0.03)
    finally:
        _open.remove(proc)
    out = proc.stdout.read() if proc.stdout else ""
    if proc.returncode == 0:
        return out.strip()
    if proc.returncode == 1:
        return ""  # cancelled
    LOG.debug("%s exited %s", cmd[0], proc.returncode)
    return None


_KDIALOG_VERB = {
    "open": "--getopenfilename",
    "opens": "--getopenfilename",
    "save": "--getsavefilename",
    "dir": "--getexistingdirectory",
}


def _command(tool: str, kind: str, opts: dict[str, Any]) -> list[str]:
    """The command line for one dialog.

    Args:
        tool: ``"kdialog"`` or ``"zenity"``.
        kind: ``"open"``, ``"opens"``, ``"save"`` or ``"dir"``.
        opts: Tk's keyword arguments.

    Returns:
        The program and its arguments.
    """
    title = str(opts.get("title") or "")
    start = _start(opts.get("initialdir"), opts.get("initialfile"))
    filters = [] if kind == "dir" else _filters(tool, opts.get("filetypes"))
    if tool == "kdialog":
        cmd = ["kdialog", _KDIALOG_VERB[kind], start, *filters]
        if kind == "opens":
            cmd += ["--multiple", "--separate-output"]
        return cmd + (["--title", title] if title else [])
    cmd = ["zenity", "--file-selection", f"--filename={start}", *filters]
    cmd += {
        "dir": ["--directory"],
        "save": ["--save", "--confirm-overwrite"],
        "opens": ["--multiple", "--separator=\n"],
    }.get(kind, [])
    return cmd + ([f"--title={title}"] if title else [])


def _native(kind: str, opts: dict[str, Any]) -> str | tuple[str, ...] | None:
    """The desktop's dialog, or None when there is none (or it failed).

    Args:
        kind: ``"open"``, ``"opens"``, ``"save"`` or ``"dir"``.
        opts: Tk's keyword arguments.

    Returns:
        The path(s) chosen, ``""``/``()`` when cancelled, or None to fall back.
    """
    tool = _native_tool()
    if tool is None:
        return None
    cmd = _command(tool, kind, opts)
    got = _run(cmd, opts.get("parent"))
    if got is None:
        return None
    if kind == "opens":
        return tuple(p for p in got.splitlines() if p)
    if kind == "save" and got:
        ext = str(opts.get("defaultextension") or "")
        if ext and not Path(got).suffix:
            got += ext if ext.startswith(".") else "." + ext
    return got


# ------------------------------------------------------------------ the API -----


def askopenfilename(**opts: Any) -> str:  # noqa: ANN401 - Tk's own signature
    """:func:`tkinter.filedialog.askopenfilename`, through the desktop's dialog on Linux."""
    got = _native("open", opts)
    if isinstance(got, str):
        return got
    _prepare_tk(opts.get("parent"))
    return _tk.askopenfilename(**opts)


def askopenfilenames(**opts: Any) -> tuple[str, ...] | str:  # noqa: ANN401
    """:func:`tkinter.filedialog.askopenfilenames`, through the desktop's dialog on Linux."""
    got = _native("opens", opts)
    if isinstance(got, tuple):
        return got
    _prepare_tk(opts.get("parent"))
    return _tk.askopenfilenames(**opts)


def asksaveasfilename(**opts: Any) -> str:  # noqa: ANN401
    """:func:`tkinter.filedialog.asksaveasfilename`, through the desktop's dialog on Linux."""
    got = _native("save", opts)
    if isinstance(got, str):
        return got
    _prepare_tk(opts.get("parent"))
    return _tk.asksaveasfilename(**opts)


def askdirectory(**opts: Any) -> str:  # noqa: ANN401
    """:func:`tkinter.filedialog.askdirectory`, through the desktop's dialog on Linux."""
    got = _native("dir", opts)
    if isinstance(got, str):
        return got
    _prepare_tk(opts.get("parent"))
    return _tk.askdirectory(**opts)
