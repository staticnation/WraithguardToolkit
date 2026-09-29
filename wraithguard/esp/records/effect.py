"""The ``ENAM`` effect block -- a port of the crate's ``types/effect.rs``.

One magic effect as it appears on a potion, an enchantment or a spell: which
effect, on what skill or attribute it acts, its range, area, duration and
magnitude range. Twenty-four bytes, and shared by every record that lists
effects -- alchemy, enchanting, spell -- so it lives on its own here.

The narrow enum widths matter: the effect id is a signed 16-bit, the skill and
attribute signed bytes, all defaulting to -1 ("none"). A wider read would
misalign every field after it.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from wraithguard.esp.enums import AttributeId2, EffectId2, EffectRange, SkillId2

#: The size of an ``ENAM`` effect subrecord, in bytes.
EFFECT_SIZE = 24


@dataclass
class Effect:
    """One magic effect (an ``ENAM`` subrecord, 24 bytes)."""

    magic_effect: EffectId2 = field(default_factory=EffectId2.default)
    skill: SkillId2 = field(default_factory=SkillId2.default)
    attribute: AttributeId2 = field(default_factory=AttributeId2.default)
    range: EffectRange = field(default_factory=EffectRange.default)
    area: int = 0
    duration: int = 0
    min_magnitude: int = 0
    max_magnitude: int = 0
