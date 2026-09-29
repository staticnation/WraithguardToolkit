"""The ``MGEF`` record -- a magic effect. A port of ``types/magiceffect.rs``.

The definition of one of the game's magic effects (fire damage, levitate, ...),
keyed by its effect id rather than a string. A thirty-six-byte ``MEDT`` block
holds its school, base cost, flags, particle colour, and particle sizing; the
rest is the icon, particle texture, and the many sounds and visual effects it
plays, plus a description.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import EffectId, EffectSchool
from wraithguard.esp.flags import MagicEffectFlags, ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class MagicEffectData:
    """The ``MEDT`` block: school, cost, flags, colour and particle sizing (36 bytes)."""

    school: EffectSchool = field(default_factory=EffectSchool.default)
    base_cost: float = 0.0
    flags: MagicEffectFlags = field(default_factory=lambda: MagicEffectFlags(0))
    color: tuple[int, int, int] = (0, 0, 0)
    speed: float = 0.0
    size: float = 0.0
    size_cap: float = 0.0


@register
@dataclass
class MagicEffect(Record):
    """A ``MGEF`` record."""

    TAG: ClassVar[bytes] = b"MGEF"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    effect_id: EffectId = field(default_factory=EffectId.default)
    icon: str = ""
    texture: str = ""
    bolt_sound: str = ""
    cast_sound: str = ""
    hit_sound: str = ""
    area_sound: str = ""
    cast_visual: str = ""
    bolt_visual: str = ""
    hit_visual: str = ""
    area_visual: str = ""
    description: str = ""
    data: MagicEffectData = field(default_factory=MagicEffectData)
