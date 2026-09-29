"""The ``DIAL`` record -- a dialogue topic. A port of ``types/dialogue.rs``.

The heading a run of ``INFO`` responses belongs to: a topic, a greeting, a voice
line, a persuasion result, or a journal (quest). Just an id and a one-byte type;
the responses that follow it in the file are separate ``INFO`` records.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import DialogueType2
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@register
@dataclass
class Dialogue(Record):
    """A ``DIAL`` record."""

    TAG: ClassVar[bytes] = b"DIAL"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    dialogue_type: DialogueType2 = field(default_factory=DialogueType2.default)
