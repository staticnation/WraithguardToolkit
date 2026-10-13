"""An OpenMW ``openmw.cfg``, read the way the viewer's engine reads it.

The reader is the Rust one (``viewer-shell/ordercore``, through ``wraithguard_native``),
shared with the viewer, so the Editor and the viewer cannot disagree about what a setup
loads. :func:`read_cfg_py` is the fallback without the Rust backend, held to the same
cases (``tests/fixtures/openmw_cfg_cases.json``).

The rules are OpenMW's documented ones ("Paths", openmw.readthedocs.io): comments start
with ``#``; names and values are trimmed; a quoted path takes ``&"`` and ``&&`` escapes
and ignores what follows the closing quote; a relative path is from its cfg's folder;
``?userdata?``-style tokens; ``config=`` chains more cfgs, breadth first, each of higher
priority; ``replace=X`` drops the lower cfgs' ``X`` values; ``data-local`` overrides every
``data=`` wherever it is written; ``groundcover=`` plugins load after the content ones.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Any, Final

#: Plugin file endings (an ``.omwscripts`` list names scripts, not records).
PLUGIN_EXTS: Final[tuple[str, ...]] = (".esm", ".esp", ".omwgame", ".omwaddon")

#: Options that gather values from every cfg.
_MANY: Final[tuple[str, ...]] = ("data", "content", "groundcover", "fallback-archive")


def is_plugin(name: str) -> bool:
    """Whether a file name is a plugin's (any case)."""
    return name.lower().endswith(PLUGIN_EXTS)


def read_cfg(
    cfg: Path, userdata: Path | None = None, userconfig: Path | None = None
) -> dict[str, Any]:
    """A setup's openmw.cfg and the cfgs it chains.

    Args:
        cfg: The openmw.cfg.
        userdata: The ``?userdata?`` folder (tests); else this machine's.
        userconfig: The ``?userconfig?`` folder (tests); else this machine's.

    Returns:
        ``files``, ``data`` (lowest priority first, ``data-local`` last),
        ``missing_data``, ``data_local``, ``content``, ``groundcover``, ``archives``,
        ``user_data``, ``resources``, ``warnings``, ``plugins`` (the load order) and
        ``located`` (each plugin with its file, or None); paths as text.

    Raises:
        OSError: When ``cfg`` cannot be read.
    """
    try:
        import wraithguard_native
    except ImportError:
        return read_cfg_py(cfg, userdata, userconfig)
    native = getattr(wraithguard_native, "openmw_cfg", None)
    if native is not None:
        return dict(native(str(cfg), userdata and str(userdata), userconfig and str(userconfig)))
    return read_cfg_py(cfg, userdata, userconfig)


def options(text: str) -> list[tuple[str, str]]:
    """A cfg's options, ``(name, value)`` in order, trimmed; comments and other lines skipped.

    Args:
        text: The cfg's text.

    Returns:
        The options.
    """
    out = []
    for line in text.splitlines():
        t = line.strip()
        if not t or t.startswith("#") or "=" not in t:
            continue
        k, v = t.split("=", 1)
        out.append((k.strip(), v.strip()))
    return out


def path_text(value: str) -> str:
    """A path value unquoted (``&"`` a quote, ``&&`` an ampersand; text after the quote dropped).

    Args:
        value: As written.

    Returns:
        The path's text.
    """
    if not value.startswith('"'):
        return value
    out: list[str] = []
    chars = iter(value[1:])
    for c in chars:
        if c == "&":
            out.append(next(chars, "&"))
        elif c == '"':
            break
        else:
            out.append(c)
    return "".join(out)


def host_tokens(cfg: Path) -> dict[str, Path | None]:
    """The token folders for a cfg: the Flatpak OpenMW's inside its sandbox, else this machine's.

    Args:
        cfg: The cfg the tokens are for.

    Returns:
        ``{"?userdata?": folder, "?userconfig?": folder, "?local?": None, "?global?": ...}``.
    """
    parts = cfg.parts
    for i in range(len(parts) - 2):
        if parts[i : i + 3] == (".var", "app", "org.openmw.OpenMW"):
            box = Path(*parts[: i + 3])
            return _tokens(box / "data" / "openmw", box / "config" / "openmw")
    home = Path.home()
    if sys.platform == "win32":
        mine = Path(os.environ.get("USERPROFILE", str(home))) / "Documents" / "My Games" / "OpenMW"
        return _tokens(mine, mine)
    if sys.platform == "darwin":
        out = _tokens(
            home / "Library" / "Application Support" / "openmw",
            home / "Library" / "Preferences" / "openmw",
        )
        out["?global?"] = Path("/Library/Application Support")
        return out
    data = os.environ.get("XDG_DATA_HOME") or str(home / ".local" / "share")
    conf = os.environ.get("XDG_CONFIG_HOME") or str(home / ".config")
    return _tokens(Path(data) / "openmw", Path(conf) / "openmw")


def _tokens(userdata: Path | None, userconfig: Path | None) -> dict[str, Path | None]:
    """The token table with these two folders."""
    return {"?userdata?": userdata, "?userconfig?": userconfig, "?local?": None, "?global?": None}


def resolve(value: str, folder: Path, tokens: dict[str, Path | None]) -> Path:
    """A path value as a path: unquoted, its token replaced, a relative one from ``folder``.

    Args:
        value: As written.
        folder: The cfg's own folder.
        tokens: :func:`host_tokens`'s table.

    Returns:
        The path.

    Raises:
        ValueError: For a token with no folder here.
    """
    text = path_text(value)
    for token, base in tokens.items():
        if text.startswith(token):
            if base is None:
                raise ValueError(f"{token} is not known here: {text}")
            rest = text[len(token) :].lstrip("/\\")
            return base / rest if rest else base
    p = Path(text)
    return p if p.is_absolute() or text.startswith(("/", "\\")) else folder / p


def read_cfg_py(
    cfg: Path, userdata: Path | None = None, userconfig: Path | None = None
) -> dict[str, Any]:
    """:func:`read_cfg` in Python (the fallback without the Rust backend).

    Args:
        cfg: The openmw.cfg.
        userdata: The ``?userdata?`` folder (tests); else this machine's.
        userconfig: The ``?userconfig?`` folder (tests); else this machine's.

    Returns:
        As :func:`read_cfg`.
    """
    tokens = host_tokens(cfg)
    if userdata is not None or userconfig is not None:
        tokens = _tokens(userdata, userconfig)
    warnings: list[str] = []
    files = [cfg]
    sources: list[tuple[Path, list[tuple[str, str]]]] = [
        (cfg.parent, options(cfg.read_text(encoding="utf-8", errors="replace")))
    ]
    seen = {cfg.resolve()}

    def push(folder_of: Path, opts: list[tuple[str, str]]) -> None:
        """Put a cfg's ``config=`` folders on the stack, the first on top."""
        found = []
        for k, v in opts:
            if k != "config":
                continue
            try:
                found.append((v, resolve(v, folder_of, tokens)))
            except ValueError as exc:
                warnings.append(f"config={v}: {exc}")
        stack.extend(reversed(found))

    # Depth first, as OpenMW's code reads them (a stack): dir1 naming dir2 then dir3, and
    # dir2 naming dir4, is dir1, dir2, dir4, dir3.
    stack: list[tuple[str, Path]] = []
    push(*sources[0])
    while stack:
        v, folder = stack.pop()
        file = folder / "openmw.cfg"
        if file.resolve() in seen:
            continue
        seen.add(file.resolve())
        try:
            text = file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            if not folder.is_dir():
                warnings.append(f"config={v}: no folder {folder}")
            continue  # a configuration folder need not hold an openmw.cfg
        opts = options(text)
        if ("replace", "config") in opts and len(sources) > 1:
            # Every cfg read before it but the first is dropped (OpenMW's rule).
            del sources[1:], files[1:]
        push(folder, opts)
        files.append(file)
        sources.append((folder, opts))
    out = _combine(sources, tokens)
    out["files"] = [str(f) for f in files]
    out["warnings"] = warnings + out["warnings"]
    return out


def _combine(
    sources: list[tuple[Path, list[tuple[str, str]]]], tokens: dict[str, Path | None]
) -> dict[str, Any]:
    """The sources (lowest priority first) as one setup."""
    floors = {
        key: max((n for n, (_f, opts) in enumerate(sources) if ("replace", key) in opts), default=0)
        for key in _MANY
    }
    warn: list[str] = []
    data: list[Path] = []
    lists: dict[str, list[str]] = {"content": [], "groundcover": [], "fallback-archive": []}
    for n, (folder, opts) in enumerate(sources):
        for k, v in opts:
            if k not in _MANY or n < floors[k] or not v:
                continue
            if k == "data":
                try:
                    data.append(resolve(v, folder, tokens))
                except ValueError as exc:
                    warn.append(f"data={v}: {exc}")
            else:
                lists[k].append(v)

    def single(key: str) -> Path | None:
        """The highest-priority cfg's value for a one-value option (an empty one sets nothing)."""
        for folder, opts in reversed(sources):
            values = [v for k, v in opts if k == key and v]
            if values:
                try:
                    return resolve(values[-1], folder, tokens)
                except ValueError as exc:
                    warn.append(f"{key}={values[-1]}: {exc}")
                    return None
        return None

    data_local = single("data-local")
    user_data = single("user-data")
    resources = single("resources")
    if data_local is not None:
        data.append(data_local)
    present = [d for d in data if d.is_dir()]
    # A plugin named twice (on both lists, or in two cases) counts once, where it first is.
    plugins: list[str] = []
    for p in [*(c for c in lists["content"] if is_plugin(c)), *lists["groundcover"]]:
        if p.lower() not in {q.lower() for q in plugins}:
            plugins.append(p)
    return {
        "data": [str(d) for d in present],
        "missing_data": [str(d) for d in data if not d.is_dir()],
        "data_local": str(data_local) if data_local is not None else None,
        "content": lists["content"],
        "groundcover": lists["groundcover"],
        "archives": lists["fallback-archive"],
        "user_data": str(user_data) if user_data is not None else None,
        "resources": str(resources) if resources is not None else None,
        "warnings": warn,
        "plugins": plugins,
        "located": locate([Path(d) for d in present], plugins),
    }


def locate(data: list[Path], names: list[str]) -> list[tuple[str, str | None]]:
    """Each name's file, in the highest-priority folder that has it (names without case).

    Args:
        data: The data folders, lowest priority first.
        names: The file names.

    Returns:
        ``(name, path or None)`` per name.
    """
    listings = [_listing(d) for d in data]
    out: list[tuple[str, str | None]] = []
    for n in names:
        hit = next((lst[n.lower()] for lst in reversed(listings) if n.lower() in lst), None)
        out.append((n, str(hit) if hit is not None else None))
    return out


def _listing(folder: Path) -> dict[str, Path]:
    """A folder's files by lower-case name (empty when it cannot be read)."""
    try:
        return {p.name.lower(): p for p in folder.iterdir() if p.is_file()}
    except OSError:
        return {}


__all__ = [
    "PLUGIN_EXTS",
    "host_tokens",
    "is_plugin",
    "locate",
    "options",
    "path_text",
    "read_cfg",
    "read_cfg_py",
    "resolve",
]
