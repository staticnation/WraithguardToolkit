"""Test in OpenMW: the patch pool written to a scratch plugin and the game started on it.

The Editor's "Test in OpenMW" button. The pool is written as a plugin of its own in a
scratch folder (the pool is kept: nothing is spent), and OpenMW is started with the
player's own configuration and that folder and plugin added last, past the main menu,
in the cell the Editor shows. Only OpenMW's documented command line is used:

- ``--data <folder>`` and ``--content <plugin>`` add to what ``openmw.cfg`` gives;
- ``--skip-menu`` goes straight into a new game (without ``--new-game``, character
  creation is bypassed);
- ``--start <cell>`` names the cell to start in (an interior, or a named exterior);
- ``--script-run <file>`` runs console commands once the game is up: ``coe x y`` for an
  exterior known only by its grid.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any

#: The scratch plugin's name: never one a load order has.
TEST_PLUGIN = "Wraithguard Test.esp"

#: What a content file of a launch setup may be.
PLUGIN_SUFFIXES = (".esp", ".esm", ".omwaddon", ".omwgame")

_GRID = re.compile(r"^\(?\s*(-?\d+)\s*,\s*(-?\d+)\s*\)?$")


def scratch_dir() -> Path:
    """The folder the test plugin is written to (made when missing)."""
    path = Path(tempfile.gettempdir()) / "wraithguard-test"
    path.mkdir(parents=True, exist_ok=True)
    return path


def _remembered_file() -> Path:
    """Where the OpenMW the player chose is remembered."""
    from wraithguard.gui import app_base_dir

    return app_base_dir() / "openmw_test.json"


def load_settings() -> dict[str, Any]:
    """What is remembered for testing: ``{exe, setups: {name: setup}, use}``."""
    try:
        got = json.loads(_remembered_file().read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {"setups": {}, "use": ""}
    if not isinstance(got, dict):
        return {"setups": {}, "use": ""}
    raw_setups, raw_use = got.get("setups"), got.get("use")
    setups: dict[str, Any] = raw_setups if isinstance(raw_setups, dict) else {}
    use: str = raw_use if isinstance(raw_use, str) else ""
    return {**got, "setups": setups, "use": use if use in setups else ""}


def save_settings(settings: dict[str, Any]) -> None:
    """Keep what :func:`load_settings` reads (quietly not, in a read-only app folder).

    Args:
        settings: The settings.
    """
    try:
        _remembered_file().write_text(json.dumps(settings, indent=1), encoding="utf-8")
    except OSError:  # a read-only app folder: asked again next time
        pass


def clean_setup(raw: object) -> dict[str, Any]:
    """A launch setup as given, checked: ``{content: [plugin names], script: text}``.

    Args:
        raw: The setup from the page.

    Returns:
        It, with the content files' names trimmed and the empty ones left out.

    Raises:
        ValueError: For a setup that is not an object, or a content file that is not a
            plugin's name.
    """
    if not isinstance(raw, dict):
        raise ValueError("a launch setup is an object")
    content = [str(c).strip() for c in raw.get("content") or [] if str(c).strip()]
    for name in content:
        if Path(name).name != name or Path(name).suffix.lower() not in PLUGIN_SUFFIXES:
            raise ValueError(f"{name} is not a plugin's name (a .esp, .esm or .omwaddon)")
    script = raw.get("script") if isinstance(raw.get("script"), str) else ""
    return {"content": content, "script": script}


def remembered_exe() -> Path | None:
    """The OpenMW executable chosen before, when it is still there."""
    raw = load_settings().get("exe")
    path = Path(raw) if isinstance(raw, str) and raw else None
    return path if path is not None and path.is_file() else None


def remember_exe(exe: Path) -> None:
    """Keep the OpenMW executable chosen, for the next test.

    Args:
        exe: The executable.
    """
    save_settings({**load_settings(), "exe": str(exe)})


def _exe_name() -> str:
    """``openmw.exe`` on Windows, ``openmw`` elsewhere."""
    return "openmw.exe" if os.name == "nt" else "openmw"


def find_openmw(given: str | None = None) -> Path | None:
    """The OpenMW executable to start.

    Args:
        given: A path the player typed: the executable, or the folder holding it.

    Returns:
        ``given`` (made the remembered one), else the remembered one, else ``openmw`` on
        the PATH, else the usual install folders; None when none is found.
    """
    if given:
        path = Path(given.strip().strip('"'))
        if path.is_dir():
            path = path / _exe_name()
        if path.is_file():
            remember_exe(path)
            return path
        return None
    found = remembered_exe()
    if found is not None:
        return found
    on_path = shutil.which("openmw")
    if on_path:
        return Path(on_path)
    places: list[Path] = []
    if os.name == "nt":
        for root in (os.environ.get("PROGRAMFILES"), os.environ.get("PROGRAMFILES(X86)")):
            if root:
                places += sorted(Path(root).glob("OpenMW*/openmw.exe"), reverse=True)
    elif sys.platform == "darwin":
        places.append(Path("/Applications/OpenMW.app/Contents/MacOS/openmw"))
    else:
        places += [Path("/usr/games/openmw"), Path("/usr/local/bin/openmw")]
    return next((p for p in places if p.is_file()), None)


def launch_args(
    exe: Path, plugin: Path, cell: str | None, setup: dict[str, Any] | None = None
) -> tuple[list[str], str | None]:
    """OpenMW's command line for a test, and the console commands it runs at start.

    Args:
        exe: OpenMW.
        plugin: The test plugin (its folder is added as a data folder).
        cell: The cell to start in: an interior's name, ``(x, y)`` for an exterior (or
            a name with it, ``Balmora (-3, -2)``), or None for the game's own start.
        setup: A launch setup (:func:`clean_setup`): its content files load before the
            test plugin, and its script runs after the move to the cell.

    Returns:
        ``(argv, script)`` - ``script`` the text of a ``--script-run`` file, or None.
    """
    argv = [str(exe), "--data", str(plugin.parent)]
    for name in (setup or {}).get("content") or []:
        argv += ["--content", name]
    argv += ["--content", plugin.name, "--skip-menu"]
    script = None
    name = (cell or "").strip()
    grid = _GRID.match(name) or re.search(r"\((-?\d+)\s*,\s*(-?\d+)\)\s*$", name)
    if grid:
        x, y = grid.groups()
        script = f"coe {int(x)} {int(y)}\n"
    elif name:
        argv += ["--start", name]
    own = ((setup or {}).get("script") or "").strip()
    if own:
        script = (script or "") + own + "\n"
    return argv, script
