"""The ``SCPT`` record -- a script. A port of ``types/script.rs``.

A Morrowind script as it is stored: a fixed-width name and a header of variable
counts and block lengths (``SCHD``), the packed local-variable name table
(``SCVR``), the compiled bytecode (``SCDT``), and the original source text
(``SCTX``). The variable table and bytecode are opaque byte blocks here -- the
library preserves them exactly; decoding them is
:mod:`wraithguard.mwscript`'s job.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

_SCHD_SIZE = 52


@dataclass
class ScriptHeader:
    """The ``SCHD`` counts: locals by type, and the bytecode/variable lengths."""

    num_shorts: int = 0
    num_longs: int = 0
    num_floats: int = 0
    bytecode_length: int = 0
    variables_length: int = 0


@register
@dataclass
class Script(Record):
    """A ``SCPT`` record."""

    TAG: ClassVar[bytes] = b"SCPT"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    header: ScriptHeader = field(default_factory=ScriptHeader)
    variables: bytes = b""
    bytecode: bytes = b""
    text: str = ""
