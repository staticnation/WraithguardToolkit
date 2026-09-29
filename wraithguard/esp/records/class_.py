"""The ``CLAS`` record -- a character class. A port of ``types/class.rs``.

A class is its two favoured attributes, its specialization, its ten
minor/major skills, and its flags and the services it offers -- all in a
sixty-byte ``CLDT`` block -- plus a name and description.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import AttributeId, SkillId, Specialization
from wraithguard.esp.flags import ClassFlags, ObjectFlags, ServiceFlags
from wraithguard.esp.record import Record, register

_SKILLS = 10


@dataclass
class ClassData:
    """The ``CLDT`` block: attributes, specialization, skills, flags, services."""

    attribute1: AttributeId = field(default_factory=AttributeId.default)
    attribute2: AttributeId = field(default_factory=AttributeId.default)
    specialization: Specialization = field(default_factory=Specialization.default)
    skills: tuple[SkillId, ...] = field(
        default_factory=lambda: tuple(SkillId.default() for _ in range(_SKILLS))
    )
    flags: ClassFlags = field(default_factory=lambda: ClassFlags(0))
    services: ServiceFlags = field(default_factory=lambda: ServiceFlags(0))


@register
@dataclass
class Class(Record):
    """A ``CLAS`` record."""

    TAG: ClassVar[bytes] = b"CLAS"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    description: str = ""
    data: ClassData = field(default_factory=ClassData)
