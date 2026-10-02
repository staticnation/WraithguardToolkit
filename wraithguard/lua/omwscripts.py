"""The ``.omwscripts`` format.

One script a line, ``<flags>: <path in the virtual file system>``; ``#`` starts a
comment line; blank lines are skipped. Flags are comma-separated: ``GLOBAL``,
``MENU``, ``CUSTOM``, ``PLAYER``, ``LOAD`` and the object types a local script
attaches to (``NPC``, ``CREATURE``, ``CONTAINER``, ...). The order of the lines is
the scripts' load order (their priority). Reference:
https://openmw.readthedocs.io/en/openmw-0.51.0/reference/lua-scripting/overview.html

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from dataclasses import dataclass, field

#: Flags OpenMW 0.51 documents. Anything else is reported, not dropped.
KNOWN_FLAGS: frozenset[str] = frozenset(
    {
        "GLOBAL",
        "MENU",
        "CUSTOM",
        "PLAYER",
        "LOAD",
        "ACTIVATOR",
        "APPARATUS",
        "ARMOR",
        "BOOK",
        "CLOTHING",
        "CONTAINER",
        "CREATURE",
        "DOOR",
        "INGREDIENT",
        "LIGHT",
        "LOCKPICK",
        "MISC_ITEM",
        "NPC",
        "POTION",
        "PROBE",
        "REPAIR",
        "WEAPON",
    }
)

#: Flags that make a script global (one instance) rather than local to objects.
GLOBAL_FLAGS: frozenset[str] = frozenset({"GLOBAL", "MENU", "LOAD"})


@dataclass(frozen=True)
class ScriptEntry:
    """One line of an ``.omwscripts`` file.

    Attributes:
        path: The script's path as written (VFS-relative, ``/`` separated).
        flags: Its flags, upper-cased, in the order written.
        source: The ``.omwscripts`` file it came from (its name).
        line: The 1-based line number in that file.
    """

    path: str
    flags: tuple[str, ...]
    source: str
    line: int

    @property
    def key(self) -> str:
        """The path as the VFS compares it: lower case, ``/`` separators."""
        return normalize_vfs_path(self.path)


@dataclass
class OmwScripts:
    """A parsed ``.omwscripts`` file.

    Attributes:
        name: The file's name, as the cfg's ``content=`` line gives it.
        entries: Its scripts, in file order.
        problems: Lines that could not be read, as ``(line, message)``.
    """

    name: str
    entries: list[ScriptEntry] = field(default_factory=list)
    problems: list[tuple[int, str]] = field(default_factory=list)


def normalize_vfs_path(path: str) -> str:
    """A VFS path as OpenMW compares it: lower case, ``/``, no leading slash.

    Args:
        path: A path as written in an ``.omwscripts`` file or found on disk.

    Returns:
        The normalized path.
    """
    return path.strip().replace("\\", "/").lstrip("/").lower()


def parse_omwscripts(text: str, name: str) -> OmwScripts:
    """Parse the text of an ``.omwscripts`` file.

    Args:
        text: The file's text.
        name: Its name, recorded on every entry.

    Returns:
        The entries in file order, and any lines that could not be read. A line
        whose flags include an unknown one is kept (OpenMW versions add flags)
        and reported as a problem.
    """
    out = OmwScripts(name=name)
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip().lstrip(chr(0xFEFF))  # a byte-order mark before the first line
        if not line or line.startswith("#"):
            continue
        head, sep, tail = line.partition(":")
        path = tail.strip()
        if not sep or not path:
            out.problems.append((number, f"not 'FLAGS: path': {line!r}"))
            continue
        # Commas in the docs; mods also separate them with spaces (PLAYER NPC: ...).
        flags = tuple(f.upper() for f in head.replace(",", " ").split())
        if not flags:
            out.problems.append((number, f"no flags before the path: {line!r}"))
            continue
        unknown = [f for f in flags if f not in KNOWN_FLAGS]
        if unknown:
            out.problems.append((number, "unknown flag(s): " + ", ".join(unknown)))
        out.entries.append(ScriptEntry(path=path, flags=flags, source=name, line=number))
    return out
