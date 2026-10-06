"""A :class:`.scan.LuaScan` as plain text.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wraithguard.lua.analysis import Finding

if TYPE_CHECKING:
    from wraithguard.lua.api import ApiVersion
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


def _native_report() -> Any:  # noqa: ANN401 - the backend's function, or None
    """The Rust backend's report (``viewer-shell/luacore/src/report.rs``), or None."""
    try:
        import wraithguard_native
    except ImportError:
        return None
    return getattr(wraithguard_native, "lua_report", None)


def render(result: LuaScan, api: ApiVersion | None = None, *, info: bool = True) -> str:
    """The scan as a report (written in Rust when the backend is built).

    Args:
        result: The scan.
        api: The API version it was checked against; the scan's own when not given.
        info: Include ``info`` findings.

    Returns:
        The report text, as :func:`render_py` writes it.
    """
    native = _native_report()
    if native is None:
        return render_py(result, api, info=info)
    api = api or result.api
    found = [(p, f.severity, f.code, f.line, f.message) for p, f in result.findings]
    for rec in result.scripts:
        if rec.info:
            found.extend(
                (rec.path, f.severity, f.code, f.line, f.message) for f in rec.info.findings
            )
        if rec.read_error:
            found.append((rec.path, "error", "UNREADABLE", 0, rec.read_error))
    header = (
        api.openmw,
        api.revision,
        len(result.omwscripts),
        len(result.scripts),
        len(result.data_dirs),
        result.api_source,
        len(result.omwaddons),
        len(result.lual_files),
    )
    scripts = [
        (
            rec.path,
            list(rec.flags),
            rec.info.interface_name if rec.info else None,
            list(rec.info.engine_handlers) if rec.info else [],
            rec.file is None,
        )
        for rec in result.scripts
    ]
    text: str = native(header, found, scripts, info)
    return text


def render_py(result: LuaScan, api: ApiVersion | None = None, *, info: bool = True) -> str:
    """The scan as a report, in Python (the fallback, and the reference).

    Args:
        result: The scan.
        api: The API version it was checked against (named in the header); the
            scan's own when not given.
        info: Include ``info`` findings.

    Returns:
        The report text.
    """
    api = api or result.api
    found = all_findings(result)
    counts = {sev: sum(1 for _, f in found if f.severity == sev) for sev in _ORDER}
    revision = f"Lua API {api.revision}" if api.revision else "Lua API revision not known"
    lines = [
        f"OpenMW Lua scripts - checked against OpenMW {api.openmw} ({revision})",
        f"{len(result.omwscripts)} .omwscripts file(s), {len(result.scripts)} script(s), "
        f"{len(result.data_dirs)} data folder(s)",
        f"{counts['error']} error(s), {counts['warn']} warning(s), {counts['info']} note(s)",
    ]
    if result.api_source:
        lines.append(result.api_source)
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
