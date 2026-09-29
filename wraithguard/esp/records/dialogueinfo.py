"""The ``INFO`` record -- one dialogue response. A port of ``types/dialogueinfo.rs``.

A single response under a topic: who says it (speaker id, race, class, faction,
cell, sex/rank), the response ``NAME`` text, an optional voice-over sound, a
result script, and the *filters* that decide when it applies. Responses form a
linked list within their topic via ``PNAM``/``NNAM``. A journal entry also
carries a quest state. Each filter is a condition (a function, a comparison and a
value); its value is a float or an integer, told apart by the subrecord that
follows it.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import (
    DialogueType,
    FilterComparison,
    FilterFunction,
    FilterType,
    Sex,
)
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.io import EspError
from wraithguard.esp.record import Record, register

_DATA_SIZE = 12


@dataclass
class DialogueData:
    """The ``DATA`` block: response type, disposition, and speaker/player rank/sex."""

    dialogue_type: DialogueType = field(default_factory=DialogueType.default)
    disposition: int = 0
    speaker_rank: int = 0
    speaker_sex: Sex = field(default_factory=Sex.default)
    player_rank: int = 0


@dataclass
class Filter:
    """One dialogue condition (an ``SCVR`` block plus its ``FLTV``/``INTV`` value)."""

    index: int = 0
    filter_type: FilterType = field(default_factory=FilterType.default)
    function: FilterFunction = field(default_factory=FilterFunction.default)
    comparison: FilterComparison = field(default_factory=FilterComparison.default)
    id: str = ""
    value: float | int = 0.0


def writer_encode(value: str) -> bytes:
    """Encode a filter id to Windows-1252 bytes (written raw, no length prefix)."""
    try:
        return value.encode("cp1252")
    except UnicodeEncodeError as exc:
        raise EspError(f"unencodable filter id {value!r}: {exc}") from exc


@register
@dataclass
class DialogueInfo(Record):
    """An ``INFO`` record."""

    TAG: ClassVar[bytes] = b"INFO"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    prev_id: str = ""
    next_id: str = ""
    data: DialogueData = field(default_factory=DialogueData)
    speaker_id: str = ""
    speaker_race: str = ""
    speaker_class: str = ""
    speaker_faction: str = ""
    speaker_cell: str = ""
    player_faction: str = ""
    sound_path: str = ""
    text: str = ""
    quest_state: str | None = None
    filters: list[Filter] = field(default_factory=list)
    script_text: str = ""

    def _last_filter(self) -> Filter:
        """The filter a trailing ``FLTV``/``INTV`` value belongs to."""
        if not self.filters:
            raise EspError("INFO: filter value without a preceding SCVR filter")
        return self.filters[-1]
