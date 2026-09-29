"""The ``SKIL`` record -- a skill definition. A port of ``types/skill.rs``.

One of the game's twenty-seven skills: which skill (``INDX``), a twenty-four-byte
``SKDT`` block (its governing attribute, its specialization, and the four
use-value gains that raise it), and a description. A skill has no id string --
it is identified by its index.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import SkillId
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

_INDX_SIZE = 4


@dataclass
class SkillData:
    """The ``SKDT`` block: governing attribute, specialization, action gains (24 bytes)."""

    governing_attribute: int = 0
    specialization: int = 0
    actions: tuple[float, ...] = (0.0, 0.0, 0.0, 0.0)


@register
@dataclass
class Skill(Record):
    """A ``SKIL`` record."""

    TAG: ClassVar[bytes] = b"SKIL"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    skill_id: SkillId = field(default_factory=SkillId.default)
    data: SkillData = field(default_factory=SkillData)
    description: str = ""
