"""The load order's Lua scripts, found and checked together.

From openmw.cfg: the ``content=`` lines that name ``.omwscripts`` files give the
scripts and their order; the ``data=`` lines (then ``data-local=``) give the folders
the virtual file system is built from, a later folder replacing an earlier one's file
of the same path, compared without case as OpenMW compares them. Each script is read
from the folder that wins and analysed (:mod:`.analysis`), and then the scripts are
checked against each other:

- a ``.omwscripts`` file or a script that no data folder has;
- one script path registered by more than one ``.omwscripts`` file;
- a script file that more than one data folder has (which one runs);
- two scripts offering the same interface where both run, or one replacing a built-in;
- events sent that no script handles;
- per-frame handlers in scripts attached to every NPC or creature.

Scripts that ``.omwaddon``/``.omwgame`` files register in LUAL records are read too
(:mod:`.lual`), in their place in the load order. Not read: scripts packed in BSA
archives - a script only found there is reported missing, and the message says why.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

from wraithguard.configurator.cfglines import cfg_line_value, unescape_cfg_value
from wraithguard.lua.analysis import Finding, ScriptInfo, analyze
from wraithguard.lua.api import API, ApiVersion, contexts_for_flags
from wraithguard.lua.lual import read_lual
from wraithguard.lua.omwscripts import OmwScripts, ScriptEntry, parse_omwscripts
from wraithguard.parallel import read_all

if TYPE_CHECKING:
    from collections.abc import Sequence

_KEY_RE = re.compile(r"^\s*([A-Za-z-]+)\s*=", re.ASCII)
_ACTOR_FLAGS = frozenset({"NPC", "CREATURE"})


@dataclass
class ScriptRecord:
    """One script of the load order.

    Attributes:
        path: Its VFS path, as first registered.
        registrations: Every ``.omwscripts`` line that registers it, in load order.
        flags: All its flags, merged over the registrations.
        providers: The data folders that have the file, in load order; the last runs.
        info: Its analysis, or None when it could not be read.
        read_error: Why it could not be read, if so.
    """

    path: str
    registrations: list[ScriptEntry] = field(default_factory=list)
    flags: tuple[str, ...] = ()
    providers: list[Path] = field(default_factory=list)
    info: ScriptInfo | None = None
    read_error: str | None = None

    @property
    def file(self) -> Path | None:
        """The file that runs: the last provider's, or None when none has it."""
        return self.providers[-1] if self.providers else None

    @property
    def contexts(self) -> frozenset[str]:
        """Where it runs (:func:`.api.contexts_for_flags`)."""
        return contexts_for_flags(self.flags)


@dataclass
class LuaScan:
    """Everything :func:`scan_load_order` found.

    Attributes:
        data_dirs: The data folders, in load order.
        omwscripts: The ``.omwscripts`` files read, in load order.
        omwaddons: ``.omwaddon``/``.omwgame`` content files (their LUAL records are read).
        lual_files: Those that register scripts in LUAL records.
        scripts: The scripts, in load order.
        findings: ``(script path or "", finding)`` for the whole load order -
            each script's own findings are on its :class:`ScriptRecord`.
        api: The API rules it was checked against.
        api_source: What the Teal checks ran against, for the report
            (:func:`.openmw_api.check_cfg`), or "".
    """

    data_dirs: list[Path] = field(default_factory=list)
    omwscripts: list[OmwScripts] = field(default_factory=list)
    omwaddons: list[str] = field(default_factory=list)
    lual_files: list[str] = field(default_factory=list)
    scripts: list[ScriptRecord] = field(default_factory=list)
    findings: list[tuple[str, Finding]] = field(default_factory=list)
    api: ApiVersion = API
    api_source: str = ""


def read_cfg_lua(cfg: Path) -> tuple[list[Path], list[str]]:
    """The data folders and content files an openmw.cfg declares.

    Args:
        cfg: The openmw.cfg.

    Returns:
        ``(data folders, content names)``, both in file order; ``data-local=``
        comes after every ``data=``, as OpenMW orders it. Relative folders are taken
        from the cfg's own folder.
    """
    data: list[Path] = []
    local: list[Path] = []
    content: list[str] = []
    for line in cfg.read_text(encoding="utf-8", errors="replace").splitlines():
        m = _KEY_RE.match(line)
        if not m:
            continue
        key = m.group(1).lower()
        raw = cfg_line_value(line) or ""
        if key in ("data", "data-local"):
            value = unescape_cfg_value(raw) if '"' in line else raw
            path = Path(value)
            if not path.is_absolute():
                path = cfg.parent / path
            (data if key == "data" else local).append(path)
        elif key == "content" and raw:
            content.append(raw)
    return data + local, content


class _Vfs:
    """Case-insensitive lookups across the data folders, with listings cached."""

    def __init__(self, dirs: Sequence[Path]) -> None:
        """Index nothing yet; folders are listed when first looked into.

        Args:
            dirs: The data folders, in load order.
        """
        self.dirs = list(dirs)
        self._listings: dict[Path, dict[str, list[Path]]] = {}

    def _listing(self, folder: Path) -> dict[str, list[Path]]:
        """A folder's entries by lower-case name (empty if it cannot be read).

        On a case-sensitive file system ``Scripts`` and ``scripts`` can both exist;
        OpenMW's VFS merges them, so both are kept.

        Args:
            folder: The folder.

        Returns:
            Lower-case name -> the entries with that name.
        """
        cached = self._listings.get(folder)
        if cached is None:
            cached = {}
            try:
                with os.scandir(folder) as it:
                    for entry in it:
                        cached.setdefault(entry.name.lower(), []).append(Path(entry.path))
            except OSError:
                pass  # a missing or unreadable data folder holds nothing
            self._listings[folder] = cached
        return cached

    def providers(self, rel: str) -> list[Path]:
        """Every data folder's file at a VFS path, in load order.

        Args:
            rel: The path, ``/`` separated (any case).

        Returns:
            The files found, one per folder that has it.
        """
        parts = [p for p in rel.replace("\\", "/").lower().split("/") if p]
        found: list[Path] = []
        for root in self.dirs:
            frontier = [root]
            for part in parts:
                frontier = [hit for cur in frontier for hit in self._listing(cur).get(part, [])]
                if not frontier:
                    break
            files = sorted(p for p in frontier if p.is_file())
            if files:
                found.append(files[0])
        return found


def _read_script(rec: ScriptRecord, api: ApiVersion) -> ScriptRecord:
    """Read and analyse one script's winning file, in place.

    Args:
        rec: The script (its ``info`` or ``read_error`` is set).
        api: The API version to check against.

    Returns:
        ``rec``.
    """
    path = rec.file
    if path is None:
        return rec
    try:
        src = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        rec.read_error = str(exc)
        return rec
    rec.info = analyze(src, rec.contexts, api)
    return rec


def scan_load_order(
    data_dirs: Sequence[Path], content: Sequence[str], api: ApiVersion = API
) -> LuaScan:
    """Find, read and check the scripts of a load order.

    Args:
        data_dirs: The data folders, in load order.
        content: The ``content=`` names, in load order.
        api: The API version to check against.

    Returns:
        The scan.
    """
    out = LuaScan(data_dirs=list(data_dirs))
    vfs = _Vfs(data_dirs)
    by_key: dict[str, ScriptRecord] = {}
    for name in content:
        low = name.lower()
        addon = low.endswith((".omwaddon", ".omwgame"))
        if addon:
            out.omwaddons.append(name)
        elif not low.endswith(".omwscripts"):
            continue
        files = vfs.providers(name)
        if not files:
            if not addon:  # a missing plugin is the load order's business, not Lua's
                _flag(out, "", "error", "MISSING_OMWSCRIPTS", 0, f"{name}: in no data folder")
            continue
        if addon:
            parsed = read_lual(files[-1], name)
            if parsed.entries or parsed.problems:
                out.lual_files.append(name)
            code = "LUAL_RECORD"
        else:
            text = files[-1].read_text(encoding="utf-8", errors="replace")
            parsed = parse_omwscripts(text, name)
            out.omwscripts.append(parsed)
            code = "OMWSCRIPTS_LINE"
        for line, message in parsed.problems:
            _flag(out, "", "warn", code, line, f"{name}: {message}")
        for entry in parsed.entries:
            rec = by_key.get(entry.key)
            if rec is None:
                rec = by_key[entry.key] = ScriptRecord(path=entry.path)
                out.scripts.append(rec)
            rec.registrations.append(entry)
            rec.flags = tuple(dict.fromkeys((*rec.flags, *entry.flags)))
    for rec in out.scripts:
        rec.providers = vfs.providers(rec.path)
    read_all(out.scripts, lambda r: _read_script(r, api))
    _cross_checks(out, api)
    return out


def scan_cfg(cfg: Path, api: ApiVersion = API) -> LuaScan:
    """:func:`scan_load_order` for an openmw.cfg.

    Args:
        cfg: The openmw.cfg.
        api: The API version to check against.

    Returns:
        The scan.
    """
    dirs, content = read_cfg_lua(cfg)
    out = scan_load_order(dirs, content, api)
    out.api = api
    return out


def _cross_checks(out: LuaScan, api: ApiVersion) -> None:
    """The checks between scripts (see the module docstring).

    Args:
        out: The scan, its scripts read (findings are added to it).
        api: The API version's rules.
    """
    for rec in out.scripts:
        if not rec.providers:
            sources = ", ".join(dict.fromkeys(e.source for e in rec.registrations))
            msg = f"registered by {sources} but in no data folder (BSAs are not searched)"
            _flag(out, rec.path, "error", "MISSING_SCRIPT", 0, msg)
        if len(rec.registrations) > 1:
            regs = "; ".join(
                f"{e.source} line {e.line} ({', '.join(e.flags)})" for e in rec.registrations
            )
            _flag(out, rec.path, "warn", "DUPLICATE_REGISTRATION", 0, f"registered twice: {regs}")
        if len(rec.providers) > 1:
            losers = ", ".join(str(p) for p in rec.providers[:-1])
            msg = f"{len(rec.providers)} data folders have it; {rec.providers[-1]} runs"
            _flag(out, rec.path, "info", "OVERRIDDEN_FILE", 0, f"{msg} (over {losers})")
        info = rec.info
        if info and any(f in _ACTOR_FLAGS for f in rec.flags):
            for name in sorted(set(info.engine_handlers) & api.per_frame):
                msg = f"{name} runs on every active NPC/creature, every frame"
                line = info.engine_handlers[name]
                _flag(out, rec.path, "warn", "PER_ACTOR_FRAME", line, msg)
    _interface_checks(out, api)
    _event_checks(out, api)


def _flag(out: LuaScan, path: str, severity: str, code: str, line: int, message: str) -> None:
    """Add a load-order finding.

    Args:
        out: The scan.
        path: The script it is about ("" for none).
        severity: ``error``, ``warn`` or ``info``.
        code: Its code.
        line: Its line (0 for the whole file).
        message: What it means.
    """
    out.findings.append((path, Finding(severity, code, line, message)))


def _interface_checks(out: LuaScan, api: ApiVersion) -> None:
    """Interfaces offered twice where both scripts run, or replacing a built-in.

    Args:
        out: The scan (findings are added to it).
        api: The API version's rules.
    """
    seen: dict[str, list[ScriptRecord]] = {}
    for rec in out.scripts:
        info = rec.info
        name = info.interface_name if info else None
        if info is None or not name:
            continue
        line = info.interface_line
        extends = "onInterfaceOverride" in info.engine_handlers
        how = "extends" if extends else "replaces (it has no onInterfaceOverride)"
        for earlier in seen.get(name, []):
            if earlier.contexts & rec.contexts:
                msg = f"interface {name!r} {how} the one from {earlier.path}"
                sev = "info" if extends else "warn"
                _flag(out, rec.path, sev, "INTERFACE_OVERRIDE", line, msg)
        if name in api.builtin_interfaces and name not in seen:
            msg = f"interface {name!r} {how} OpenMW's built-in one"
            _flag(out, rec.path, "info", "BUILTIN_OVERRIDE", line, msg)
        seen.setdefault(name, []).append(rec)


def _event_checks(out: LuaScan, api: ApiVersion) -> None:
    """Events sent that no script handles.

    Args:
        out: The scan (findings are added to it).
        api: The API version's rules.
    """
    handled: set[str] = set(api.builtin_events)
    for rec in out.scripts:
        if rec.info:
            handled.update(rec.info.event_handlers)
    for rec in out.scripts:
        if not rec.info:
            continue
        for event, line in sorted(rec.info.sent_events.items()):
            if event not in handled:
                msg = (
                    f"sends {event!r}, which no script handles that this can see (a typo, a "
                    "missing mod, or handlers built at run time)"
                )
                _flag(out, rec.path, "warn", "UNHANDLED_EVENT", line, msg)
