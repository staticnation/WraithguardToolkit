"""The ``TES3`` record -- the plugin header. A port of ``types/header.rs``.

Every plugin opens with this: its format version, whether it is a master, a
plugin or a save, the author and description shown in the launcher, the number of
records that follow, and the masters it depends on. The author and description
are fixed-width fields (32 and 256 bytes); each master is a ``MAST`` name paired
with a ``DATA`` giving that master file's size, which the engine checks.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import FileType
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

_HEDR_SIZE = 300
#: The format version Morrowind's own files carry.
_DEFAULT_VERSION = 1.3


@register
@dataclass
class Header(Record):
    """A ``TES3`` record: the plugin header and its master list."""

    TAG: ClassVar[bytes] = b"TES3"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    version: float = _DEFAULT_VERSION
    file_type: FileType = field(default_factory=lambda: FileType.Esp)
    author: str = ""
    description: str = ""
    num_objects: int = 0
    masters: list[tuple[str, int]] = field(default_factory=list)
