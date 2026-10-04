"""The setup's own OpenMW Lua API, and scripts checked against it by the Teal compiler.

Every OpenMW install documents its Lua API: ``resources/lua_api/openmw/*.lua`` for the
``openmw.*`` packages and the Lua files under ``resources/vfs/`` for the built-in
interfaces and ``openmw_aux.*``, in Lua Development Tools comments. Those files are
GPLv3, so the toolkit ships none of them: it finds the install (:func:`find_resources`)
and reads them there, at run time, through the Rust backend (``native/src/lua``), which
turns them into Teal declarations (``.d.tl``) in a temporary folder and checks every
script with the Teal compiler (tl 0.24.8, embedded with htl - no Lua install needed).

What that adds to :mod:`.scan`'s checks, per script:

- ``API_TYPO``: a key a documented type does not have, close to one it does
  (``self.recrdId``) - a warning, with the likely name;
- ``API_UNDOCUMENTED``: any other key the documentation does not list - a note, since
  the documentation leaves some real members out (``time.day``);
- ``API_INTERNAL``: an undocumented ``_name`` - OpenMW's own scripts use these;
- ``TOO_MANY_ARGS``: more arguments than a function takes (fewer is not reported: the
  documentation rarely says which parameters are optional);
- ``UNKNOWN_GLOBAL``: a global that is neither Lua's, OpenMW's, nor assigned anywhere;
- ``REQUIRE_NOT_FOUND``: an ``openmw.*`` package this OpenMW does not have, or a module
  no data folder has;
- ``UNUSED_LOCAL``: a local variable or function never used - a note;
- ``TEAL``: for a ``.tl`` file (a mod written in Teal), every type error, since its author
  asked for them.

With no install to read, the same checks run against stubs for the known packages:
everything but the API ones. A folder of OpenMW's own published declarations
(``teal_declarations``) can be given instead of an install.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import difflib
import os
import re
import shutil
import sys
import tempfile
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import TYPE_CHECKING, Any

from wraithguard.configurator.cfglines import cfg_line_value, unescape_cfg_value
from wraithguard.logging_setup import get_logger
from wraithguard.lua.analysis import Finding, bound_names
from wraithguard.lua.api import API, ApiVersion

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

    from wraithguard.lua.parser import Node
    from wraithguard.lua.scan import LuaScan

LOG = get_logger(__name__)

_KEY_RE = re.compile(r"^\s*resources\s*=", re.IGNORECASE)
_INVALID_KEY_RE = re.compile(
    r"^invalid key '([^']*)' in .*?\b(?:type|of type) (?:record )?([A-Za-z0-9_]+__[A-Za-z0-9_]+)\b"
)
_ARITY_RE = re.compile(
    r"given (\d+), expects (?:at least \d+ and at most (\d+)|(\d+) or (\d+)|(?:at most )?(\d+))"
)
_UNKNOWN_RE = re.compile(r"^unknown variable: ([A-Za-z_][A-Za-z0-9_]*)")
_MODULE_RE = re.compile(r"module not found: '([^']+)'")
#: A leading verb: ``getX`` and ``setX`` are two functions, not a typo of each other.
_VERB_RE = re.compile(r"^(get|set|is|has|add|remove|can|to)(?=[A-Z_])")

#: Where OpenMW installs put ``resources``, tried after the cfg and the environment.
_INSTALL_GLOBS: tuple[str, ...] = (
    "C:/Program Files/OpenMW*/resources",
    "C:/Program Files (x86)/OpenMW*/resources",
    "C:/Games/OpenMW*/resources",
    "/usr/share/games/openmw/resources",
    "/usr/share/openmw/resources",
    "/usr/local/share/games/openmw/resources",
    "/usr/local/share/openmw/resources",
    "/var/lib/flatpak/app/org.openmw.OpenMW/current/active/files/share/games/openmw/resources",
    "~/.local/share/flatpak/app/org.openmw.OpenMW/current/active/files/share/games/openmw/resources",
    "/Applications/OpenMW.app/Contents/Resources/resources",
)


def is_resources(path: Path) -> bool:
    """Whether a folder is an OpenMW ``resources`` folder with a Lua API.

    Args:
        path: The folder.

    Returns:
        True when it has ``lua_api/openmw``.
    """
    return (path / "lua_api" / "openmw").is_dir()


def find_resources(cfg: Path | None = None, explicit: Path | None = None) -> Path | None:
    """The ``resources`` folder of the OpenMW install a setup runs on.

    Tried in order: ``explicit``; ``WG_OPENMW_RESOURCES``; a ``resources=`` line in the
    cfg (relative to the cfg's folder), and the folder OpenMW's global openmw.cfg names
    when the cfg's own ``data=`` folders sit in an install; then the usual install places
    (the newest first). Only a folder with ``lua_api/openmw`` counts.

    Args:
        cfg: The setup's openmw.cfg.
        explicit: A folder the user chose: ``resources`` itself, or the install folder
            holding it.

    Returns:
        The folder, or None.
    """
    tried: list[Path] = []
    if explicit is not None:
        tried += [explicit, explicit / "resources"]
    env = os.environ.get("WG_OPENMW_RESOURCES")
    if env:
        tried.append(Path(env))
    if cfg is not None:
        try:
            for line in cfg.read_text(encoding="utf-8", errors="replace").splitlines():
                if _KEY_RE.match(line):
                    raw = unescape_cfg_value(cfg_line_value(line) or "")
                    p = Path(raw)
                    tried.append(p if p.is_absolute() else cfg.parent / p)
        except OSError:
            pass  # an unreadable cfg names no folder; the other places are still tried
    for pattern in _INSTALL_GLOBS:
        base = Path(pattern).expanduser()
        if "*" in pattern:
            anchor = Path(base.anchor)
            rel = str(base.relative_to(anchor))
            tried += sorted(anchor.glob(rel), reverse=True)
        else:
            tried.append(base)
    if sys.platform == "darwin":
        tried.append(Path("~/Applications/OpenMW.app/Contents/Resources/resources").expanduser())
    for p in tried:
        if is_resources(p):
            return p
    return None


def _native() -> Any:  # noqa: ANN401 - the extension module, or None
    """The Rust backend, or None when it is not built."""
    try:
        import wraithguard_native  # optional for this package
    except ImportError:
        return None
    return wraithguard_native if hasattr(wraithguard_native, "lua_check") else None


@dataclass
class OpenmwDocs:
    """What an install's Lua API documentation says.

    Attributes:
        resources: The ``resources`` folder it was read from.
        version: The install's version (``resources/version``), or "".
        modules: Each documented module: ``name``, ``require``, ``interface``,
            ``contexts``, ``file``, ``fields``, ``functions``, ``types``.
        members: Each declared type's member names, by its Teal name.
    """

    resources: Path
    version: str
    modules: list[dict[str, Any]] = field(default_factory=list)
    members: dict[str, list[str]] = field(default_factory=dict)

    @property
    def packages(self) -> list[str]:
        """The packages a script can require, sorted."""
        return sorted({m["require"] for m in self.modules if not m["interface"]})

    @property
    def interfaces(self) -> list[str]:
        """The built-in interfaces it documents."""
        return sorted(m["interface"] for m in self.modules if m["interface"])


def read_docs(resources: Path) -> OpenmwDocs | None:
    """Read an install's Lua API documentation (through the Rust backend).

    Args:
        resources: The install's ``resources`` folder.

    Returns:
        What it says, or None without the backend or a documented API there.
    """
    native = _native()
    if native is None:
        return None
    got = native.lua_api(resources)
    if got is None:
        return None
    return OpenmwDocs(resources, got["version"] or "", list(got["modules"]), dict(got["members"]))


#: The documentation's context names -> :mod:`.api`'s. Its ``local`` is any local
#: script, the player's among them.
_DOC_CONTEXTS = {
    "global": {"global"},
    "local": {"local", "player"},
    "player": {"player"},
    "menu": {"menu"},
    "load": {"load"},
}


def api_from_docs(docs: OpenmwDocs, base: ApiVersion = API) -> ApiVersion:
    """:data:`.api.API` brought to an install's documentation.

    The packages and where each may be required, and the built-in interfaces, are the
    install's; engine handlers and built-in events, which the documentation does not
    list in a form to read, stay as ``base`` has them.

    Args:
        docs: The install's documentation.
        base: What to start from.

    Returns:
        The rules for that install.
    """
    every = frozenset({"global", "menu", "load", "player", "local"})
    packages: dict[str, frozenset[str]] = {}
    for m in docs.modules:
        if m["interface"] or not m["require"].startswith("openmw."):
            continue
        ctx: set[str] = set()
        for c in m["contexts"]:
            ctx |= _DOC_CONTEXTS.get(c, set())
        if ctx and frozenset(ctx) != every:
            packages[m["require"]] = frozenset(ctx)
    version = docs.version or base.openmw
    return replace(
        base,
        openmw=version,
        revision=base.revision if version == base.openmw else 0,
        packages=packages or dict(base.packages),
        builtin_interfaces=base.builtin_interfaces | frozenset(docs.interfaces),
    )


@dataclass
class TealSetup:
    """The declarations the checks run against.

    Attributes:
        folder: Where the ``.d.tl`` files are.
        source: ``openmw`` (written from an install), ``declarations`` (a folder the user
            gave) or ``stubs``.
        description: For the report's header.
        members: Each declared type's member names (empty but for ``openmw``).
        owned: Whether :meth:`close` deletes ``folder``.
        vfs: The install's ``resources/vfs``, whose built-in modules scripts may require.
    """

    folder: Path
    source: str
    description: str
    members: dict[str, list[str]] = field(default_factory=dict)
    owned: bool = False
    vfs: Path | None = None

    def close(self) -> None:
        """Delete the folder when it is a temporary one."""
        if self.owned:
            shutil.rmtree(self.folder, ignore_errors=True)


def teal_setup(
    docs: OpenmwDocs | None,
    declarations: Path | None = None,
    packages: Iterable[str] = (),
    out: Path | None = None,
) -> TealSetup | None:
    """Get the declarations ready.

    Args:
        docs: The install's documentation, if one was found.
        declarations: A folder of ready ``.d.tl`` files to use instead (OpenMW's
            published ``teal_declarations``).
        packages: The packages to stub when there is neither.
        out: Write into this folder (kept) rather than a temporary one.

    Returns:
        The setup, or None without the Rust backend.
    """
    native = _native()
    if native is None:
        return None
    if declarations is not None:
        return TealSetup(declarations, "declarations", f"the Teal declarations in {declarations}")
    folder = out if out is not None else Path(tempfile.mkdtemp(prefix="wg_teal_decl_"))
    got = native.lua_write_declarations(
        folder, docs.resources if docs else None, sorted(set(packages) | _STUB_PACKAGES)
    )
    if got["source"] == "openmw" and docs is not None:
        ver = f"OpenMW {docs.version}" if docs.version else "OpenMW"
        desc = f"{ver}'s documented Lua API (read from {docs.resources})"
        vfs = docs.resources / "vfs"
        return TealSetup(
            folder,
            "openmw",
            desc,
            dict(docs.members),
            owned=out is None,
            vfs=vfs if vfs.is_dir() else None,
        )
    return TealSetup(folder, "stubs", "no OpenMW install found: API checks off", owned=out is None)


#: The packages stubbed when no install is read: OpenMW's, as of 0.51.
_STUB_PACKAGES = frozenset(
    {
        "openmw.ambient",
        "openmw.animation",
        "openmw.async",
        "openmw.camera",
        "openmw.content",
        "openmw.core",
        "openmw.debug",
        "openmw.input",
        "openmw.interfaces",
        "openmw.markup",
        "openmw.menu",
        "openmw.nearby",
        "openmw.postprocessing",
        "openmw.self",
        "openmw.storage",
        "openmw.types",
        "openmw.ui",
        "openmw.util",
        "openmw.vfs",
        "openmw.world",
        "openmw_aux.calendar",
        "openmw_aux.time",
        "openmw_aux.ui",
        "openmw_aux.util",
    }
)


def _typo_of(key: str, names: Sequence[str]) -> str | None:
    """The documented name ``key`` is likely a typo of, if any.

    Args:
        key: The name a script used.
        names: The type's documented member names.

    Returns:
        The closest name when it is close and not merely the same word under another
        verb (``getConsoleMode`` is not a typo of ``setConsoleMode``), else None.
    """
    stem = _VERB_RE.sub("", key)
    for cand in difflib.get_close_matches(key, names, n=3, cutoff=0.75):
        if stem != key and _VERB_RE.sub("", cand) == stem:
            continue
        return cand
    return None


def _type_label(tid: str) -> str:
    """A declared type's Teal name, for a message.

    ``nearby__nearby`` is ``openmw.nearby``, ``core__GameObject`` is ``openmw.core
    GameObject``, ``aux_time__time`` is ``openmw_aux.time``, ``I_AI__AI`` is ``I.AI``.

    Args:
        tid: The Teal name.

    Returns:
        The label.
    """
    key, _, name = tid.partition("__")
    if key.startswith("I_"):
        package, short = f"I.{key[2:]}", key[2:]
    elif key.startswith("aux_"):
        package, short = f"openmw_aux.{key[4:]}", key[4:]
    else:
        package, short = f"openmw.{key}", key
    return package if name.lower() == short.lower() else f"{package} {name}"


def _too_many(message: str) -> bool:
    """Whether an arity message is about too many arguments (not too few)."""
    m = _ARITY_RE.search(message)
    if not m:
        return False
    given = int(m.group(1))
    most = next((int(g) for g in (m.group(2), m.group(4), m.group(5)) if g), None)
    return most is not None and given > most


def findings_for(
    diags: Sequence[dict[str, Any]], *, teal: bool, tree: Node | None, setup: TealSetup
) -> list[Finding]:
    """Teal's diagnostics for one file, as findings (see the module docstring).

    Args:
        diags: ``lua_check``'s diagnostics for it.
        teal: Whether the file is Teal (``.tl``): every type error then counts.
        tree: The script's syntax tree (for the names it binds).
        setup: The declarations it was checked against.

    Returns:
        The findings, in line order.
    """
    bound = bound_names(tree)
    out: list[Finding] = []
    seen: set[tuple[str, int, str]] = set()

    def add(sev: str, code: str, line: int, msg: str) -> None:
        """Add a finding once.

        Args:
            sev: Its severity.
            code: Its code.
            line: Its line.
            msg: What it says.
        """
        if (code, line, msg) not in seen:
            seen.add((code, line, msg))
            out.append(Finding(sev, code, line, msg))

    for d in diags:
        kind, msg, line = d["kind"], d["message"], int(d["line"])
        if kind == "invalid_key":
            m = _INVALID_KEY_RE.match(msg)
            if not m:
                # A record Teal made up from a table in the script: its inference, not
                # the API.
                if teal:
                    add("warn", "TEAL", line, msg)
                continue
            key, tid = m.group(1), m.group(2)
            label = _type_label(tid)
            close = _typo_of(key, setup.members.get(tid, []))
            if close:
                add("warn", "API_TYPO", line, f"{label} has no {key!r} - did you mean {close!r}?")
            elif key.startswith("_"):
                add("info", "API_INTERNAL", line, f"{label}: {key} is internal to OpenMW")
            else:
                add(
                    "info", "API_UNDOCUMENTED", line, f"{label}: {key} is not in the documented API"
                )
        elif kind == "arity":
            if _too_many(msg) or teal:
                add("warn", "TOO_MANY_ARGS" if _too_many(msg) else "TEAL", line, msg)
        elif kind == "unknown_variable":
            m = _UNKNOWN_RE.match(msg)
            if m and m.group(1) not in bound:
                add(
                    "warn",
                    "UNKNOWN_GLOBAL",
                    line,
                    f"{m.group(1)} is not defined (a typo, or a global set elsewhere)",
                )
        elif kind == "module_not_found":
            m = _MODULE_RE.search(msg)
            name = m.group(1) if m else msg
            if name.startswith(("openmw.", "openmw_aux.")):
                if setup.source != "stubs":
                    add(
                        "error",
                        "REQUIRE_NOT_FOUND",
                        line,
                        f"{name} is not a package of this OpenMW",
                    )
            else:
                add(
                    "warn",
                    "REQUIRE_NOT_FOUND",
                    line,
                    f"require({name!r}): no data folder has that module",
                )
        elif kind in ("unused_variable", "unused_function"):
            # "unused variable x: <type> (inferred at ...)" -> "unused variable x".
            add("info", "UNUSED_LOCAL", line, msg.split(":", 1)[0])
        elif teal and kind in ("type", "syntax"):
            add("error" if d["severity"] == "error" else "warn", "TEAL", line, msg)
    out.sort(key=lambda f: f.line)
    return out


def add_teal_findings(scan: LuaScan, setup: TealSetup) -> int:
    """Check every script of a scan with Teal and add the findings to its records.

    A script with a ``.tl`` beside it is checked as that ``.tl`` (its typed source);
    any other as the ``.lua`` that runs. The data folders, in load order, answer the
    scripts' requires of each other.

    Args:
        scan: The scan (its records' findings are added to).
        setup: The declarations.

    Returns:
        How many scripts were checked.
    """
    native = _native()
    if native is None:
        return 0
    jobs: list[tuple[Any, Path, bool]] = []
    for rec in scan.scripts:
        f = rec.file
        if f is None or rec.info is None or rec.info.tree is None:
            continue
        tl = f.with_suffix(".tl")
        if tl.is_file():
            jobs.append((rec, tl, True))
        else:
            jobs.append((rec, f, False))
    if not jobs:
        return 0
    # Lowest priority first: OpenMW's own files, the data folders, the declarations.
    include = [*([setup.vfs] if setup.vfs else []), *scan.data_dirs, setup.folder]
    results = native.lua_check([j[1] for j in jobs], include)
    for (rec, path, is_teal), res in zip(jobs, results, strict=True):
        if isinstance(res, str):
            LOG.debug("teal could not check %s: %s", path, res)
            continue
        rec.info.findings.extend(findings_for(res, teal=is_teal, tree=rec.info.tree, setup=setup))
    return len(jobs)


def check_cfg(
    cfg: Path,
    *,
    resources: Path | None = None,
    declarations: Path | None = None,
    teal: bool = True,
    write_declarations: Path | None = None,
) -> LuaScan:
    """The whole Lua check of a setup: the install's API, the scan, then Teal.

    Args:
        cfg: The setup's openmw.cfg.
        resources: The OpenMW install's ``resources`` folder (or the install folder);
            found by :func:`find_resources` when not given.
        declarations: A folder of ready Teal declarations to check against instead.
        teal: Run the Teal checks (they need the Rust backend).
        write_declarations: Keep the declarations written from the install in this
            folder (for a Teal project's ``include_dir``) rather than a temporary one.

    Returns:
        The scan, with :attr:`.scan.LuaScan.api` and ``api_source`` filled in.
    """
    from wraithguard.lua.scan import scan_cfg  # here: keeps scan free of this module

    found = find_resources(cfg, resources)
    docs = read_docs(found) if found else None
    api = api_from_docs(docs) if docs else API
    scan = scan_cfg(cfg, api)
    scan.api = api
    if not teal:
        scan.api_source = "Teal checks off"
        return scan
    setup = teal_setup(docs, declarations, out=write_declarations)
    if setup is None:
        scan.api_source = "Teal checks off: the Rust backend (native/) is not built"
        return scan
    try:
        n = add_teal_findings(scan, setup)
        scan.api_source = f"Teal checked {n} script(s) against {setup.description}"
    finally:
        setup.close()
    return scan


__all__ = [
    "OpenmwDocs",
    "TealSetup",
    "add_teal_findings",
    "api_from_docs",
    "check_cfg",
    "find_resources",
    "findings_for",
    "is_resources",
    "read_docs",
    "teal_setup",
]
