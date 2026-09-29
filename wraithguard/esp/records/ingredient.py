"""The ``INGR`` record -- an alchemy ingredient. A port of ``types/ingredient.rs``.

The item strings plus a fifty-six-byte ``IRDT`` block: weight, value, and three
parallel arrays of four -- the effects the ingredient can have, and the skill or
attribute each acts on. Every array element is a signed 32-bit id defaulting to
-1, so an unused slot is ``None``.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import AttributeId, EffectId, SkillId
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

_SLOTS = 4


def _four_effects() -> tuple[EffectId, ...]:
    """The four default effect ids an ingredient's ``IRDT`` block starts with."""
    return (EffectId.default(),) * _SLOTS


def _four_skills() -> tuple[SkillId, ...]:
    """The four default skill ids for an ingredient's effect slots."""
    return (SkillId.default(),) * _SLOTS


def _four_attributes() -> tuple[AttributeId, ...]:
    """The four default attribute ids for an ingredient's effect slots."""
    return (AttributeId.default(),) * _SLOTS


@dataclass
class IngredientData:
    """The ``IRDT`` block: weight, value, and the effect/skill/attribute arrays."""

    weight: float = 0.0
    value: int = 0
    effects: tuple[EffectId, ...] = field(default_factory=_four_effects)
    skills: tuple[SkillId, ...] = field(default_factory=_four_skills)
    attributes: tuple[AttributeId, ...] = field(default_factory=_four_attributes)


@register
@dataclass
class Ingredient(Record):
    """An ``INGR`` record."""

    TAG: ClassVar[bytes] = b"INGR"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    data: IngredientData = field(default_factory=IngredientData)
