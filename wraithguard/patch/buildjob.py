"""The viewer's Build patch and Test in OpenMW links, less the threads and the game.

What ``gui/editorlink.py``'s ``editBuildCheck``, ``editBuild``, ``editBuildStatus``,
``editTestSetups`` and ``editPreviewPatch`` decide: where a patch may go, whether a build
may start, what the page is told while it runs, and the launch setups kept. Ported to
``viewer-shell/editcore/src/buildjob.rs`` (port step 4.1, slice 6b); both are run against
``tests/fixtures/buildjob_cases.json``.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any, Final

from wraithguard.i18n import gettext as _

if TYPE_CHECKING:
    from collections.abc import Iterable

#: What a patch may be called.
PATCH_SUFFIXES: Final = (".esp", ".esm", ".omwaddon")

#: How a build treats a file already there.
MODES: Final = ("new", "append", "replace")


def patch_target(req: dict[str, Any]) -> Path:
    """Where a request says a patch goes, checked.

    Args:
        req: ``{path, ...}``.

    Returns:
        The path, trimmed.

    Raises:
        ValueError: For a path that is not a plugin's, not absolute, or in a folder that
            does not exist.
    """
    raw = req.get("path")
    if not isinstance(raw, str) or not raw.strip():
        raise ValueError(_("Choose where the patch goes"))
    target = Path(raw.strip())
    if not target.is_absolute():
        raise ValueError(_("The patch needs a full path, folder and all"))
    if target.suffix.lower() not in PATCH_SUFFIXES:
        raise ValueError(_("A patch is a plugin: name it .esp (or .omwaddon)"))
    if not target.parent.is_dir():
        raise ValueError(_("%(folder)s is not a folder") % {"folder": target.parent})
    return target


def build_check(target: Path, order: Iterable[str]) -> dict[str, Any]:
    """``editBuildCheck``'s answer: whether the file is there, and the load order has it.

    Args:
        target: The checked path (:func:`patch_target`).
        order: The load order's plugin names.

    Returns:
        ``{path, exists, inOrder}``.
    """
    names = {n.lower() for n in order}
    return {"path": str(target), "exists": target.exists(), "inOrder": target.name.lower() in names}


def build_mode(req: dict[str, Any], target: Path, running: dict[str, Any] | None) -> str:
    """How ``editBuild`` writes, or why it will not start.

    Args:
        req: ``{mode?}``: ``new`` (the default), ``append`` or ``replace``.
        target: The checked path.
        running: The last build's state, if any.

    Returns:
        The mode.

    Raises:
        ValueError: For a bad mode, a file that exists with no ``append`` or ``replace``,
            or a build already running.
    """
    mode = req.get("mode") or "new"
    if mode not in MODES:
        raise ValueError("bad mode")
    if mode == "new" and target.exists():
        raise ValueError(_("%(name)s exists: choose Append or Replace") % {"name": target.name})
    refuse_running(running)
    return str(mode)


def refuse_running(running: dict[str, Any] | None) -> None:
    """Refuses a second build while one runs.

    Raises:
        ValueError: When ``running`` is still running.
    """
    if running is not None and running["state"] == "running":
        raise ValueError(_("A patch is being written already"))


def new_state(running: dict[str, Any] | None) -> dict[str, Any]:
    """A build's state as it starts: the next job number, nothing said yet."""
    return {
        "job": (running or {}).get("job", 0) + 1,
        "state": "running",
        "lines": [],
        "result": None,
        "error": None,
    }


def build_status(state: dict[str, Any] | None, req: dict[str, Any]) -> dict[str, Any]:
    """``editBuildStatus``'s answer.

    Args:
        state: The build's state, if any.
        req: ``{job, from}``: the job, and how many report lines the page has.

    Returns:
        ``{job, state, lines, total, result, error}`` - ``lines`` from ``from`` on.

    Raises:
        ValueError: For a job that is not the current one.
    """
    if state is None or req.get("job") != state["job"]:
        raise ValueError(_("No such patch build"))
    start = req.get("from") or 0
    if not isinstance(start, int) or start < 0:
        start = 0
    return {
        "job": state["job"],
        "state": state["state"],
        "lines": state["lines"][start:],
        "total": len(state["lines"]),
        "result": state["result"],
        "error": state["error"],
    }


def update_setups(settings: dict[str, Any], req: dict[str, Any]) -> bool:
    """``editTestSetups``: the launch setups replaced and the one used chosen, in place.

    Args:
        settings: What :func:`.testrun.load_settings` read.
        req: ``{setups?, use?}``.

    Returns:
        Whether the request changed anything to keep.

    Raises:
        ValueError: For setups that are not an object, or a setup
            :func:`.testrun.clean_setup` refuses.
    """
    from wraithguard.patch.testrun import clean_setup

    if "setups" in req:
        raw = req["setups"]
        if not isinstance(raw, dict):
            raise ValueError("setups is an object")
        settings["setups"] = {
            str(k).strip(): clean_setup(v) for k, v in raw.items() if str(k).strip()
        }
    if "use" in req:
        use = req["use"] if isinstance(req["use"], str) else ""
        settings["use"] = use if use in settings["setups"] else ""
    elif settings["use"] not in settings["setups"]:
        settings["use"] = ""
    return "setups" in req or "use" in req


def test_request(
    req: dict[str, Any], exe_given: str | None, exe: Path | None, settings: dict[str, Any]
) -> tuple[Path, str | None, dict[str, Any] | None]:
    """What ``editTestRun`` starts the game with, or why it cannot.

    Args:
        req: ``{cell?, exe?, setup?}``.
        exe_given: The OpenMW the request names, if it is text.
        exe: The OpenMW found (that one, or one known), if any.
        settings: The test settings (:func:`.testrun.load_settings`).

    Returns:
        ``(exe, cell, setup)``: OpenMW, the cell to start in (None: the game's own start) and the
        launch setup (the one asked for, else the one last chosen; None: none).

    Raises:
        ValueError: ``needExe: ...`` when OpenMW is not found, or for a setup that is not
            kept.
    """
    if exe is None:
        raise ValueError(
            "needExe: "
            + (
                _("%(path)s is not OpenMW") % {"path": exe_given}
                if exe_given
                else _("Where is OpenMW? Choose openmw.exe (or the folder it is in)")
            )
        )
    cell = req.get("cell") if isinstance(req.get("cell"), str) else None
    name = req.get("setup") if isinstance(req.get("setup"), str) else settings["use"]
    setup = settings["setups"].get(name) if name else None
    if name and setup is None:
        raise ValueError(_("No launch setup is called %(name)s") % {"name": name})
    return exe, cell, setup


def preview_target(req: dict[str, Any]) -> Path:
    """``editPreviewPatch``'s plugin: a written one.

    Raises:
        ValueError: For a path :func:`patch_target` refuses, or no file there yet.
    """
    target = patch_target(req)
    if not target.is_file():
        raise ValueError(_("%(name)s is not written yet") % {"name": target.name})
    return target


__all__ = [
    "MODES",
    "PATCH_SUFFIXES",
    "build_check",
    "build_mode",
    "build_status",
    "new_state",
    "patch_target",
    "preview_target",
    "refuse_running",
    "test_request",
    "update_setups",
]
