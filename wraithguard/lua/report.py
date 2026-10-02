"""A :class:`.scan.LuaScan` as plain text.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from wraithguard.lua.analysis import Finding
from wraithguard.lua.api import API, ApiVersion

if TYPE_CHECKING:
    from wraithguard.lua.scan import LuaScan

_ORDER = {"error": 0, "warn": 1, "info": 2}
_MARK = {"error": "ERROR", "warn": "warn ", "info": "info "}


def all_findings(result: LuaScan) -> list[tuple[str, Finding]]:
    """Every finding - the load order's and each script's - most serious first.

    Args:
        result: The scan.

    Returns:
        ``(script path or "", finding)``, sorted by severity, then path, then line.
    """
    found = list(result.findings)
    for rec in result.scripts:
        if rec.info:
            found.extend((rec.path, f) for f in rec.info.findings)
        if rec.read_error:
            found.append((rec.path, Finding("error", "UNREADABLE", 0, rec.read_error)))
    found.sort(key=lambda pf: (_ORDER.get(pf[1].severity, 3), pf[0].lower(), pf[1].line))
    return found


def render(result: LuaScan, api: ApiVersion = API, *, info: bool = True) -> str:
    """The scan as a report.

    Args:
        result: The scan.
        api: The API version it was checked against (named in the header).
        info: Include ``info`` findings.

    Returns:
        The report text.
    """
    found = all_findings(result)
    counts = {sev: sum(1 for _, f in found if f.severity == sev) for sev in _ORDER}
    lines = [
        f"OpenMW Lua scripts - checked against OpenMW {api.openmw} (Lua API {api.revision})",
        f"{len(result.omwscripts)} .omwscripts file(s), {len(result.scripts)} script(s), "
        f"{len(result.data_dirs)} data folder(s)",
        f"{counts['error']} error(s), {counts['warn']} warning(s), {counts['info']} note(s)",
    ]
    if result.omwaddons:
        lines.append(
            f"LUAL records read from {len(result.omwaddons)} .omwaddon file(s); "
            f"{len(result.lual_files)} register scripts."
        )
    lines.append("")
    for path, f in found:
        if f.severity == "info" and not info:
            continue
        where = path or "(load order)"
        if f.line:
            where += f":{f.line}"
        lines.append(f"{_MARK.get(f.severity, f.severity)} {f.code:<22} {where}")
        lines.append(f"      {f.message}")
    lines.append("")
    lines.append("Scripts, in load order:")
    for rec in result.scripts:
        flags = ", ".join(rec.flags)
        tail = ""
        if rec.info and rec.info.interface_name:
            tail += f"  interface {rec.info.interface_name}"
        if rec.info and rec.info.engine_handlers:
            tail += "  [" + ", ".join(sorted(rec.info.engine_handlers)) + "]"
        if rec.file is None:
            tail += "  (missing)"
        lines.append(f"  {rec.path}  ({flags}){tail}")
    return "\n".join(lines) + "\n"
