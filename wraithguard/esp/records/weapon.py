"""The ``WEAP`` record -- a weapon. A port of the crate's ``types/weapon.rs``.

Two pieces, exactly as the crate splits them: :class:`Weapon`, the record with
its string subrecords (id, model, name, script, icon, enchantment) and its data
block; and :class:`WeaponData`, the fixed thirty-two-byte ``WPDT`` payload of
its numbers (weights, values, damages) and its type and flags. The load reads
the subrecords by tag; the save writes them back in the crate's order, omitting
the optional strings when empty -- so a weapon read and written is byte-for-byte
what it was.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import WeaponType
from wraithguard.esp.flags import ObjectFlags, WeaponFlags
from wraithguard.esp.record import Record, register


@dataclass
class WeaponData:
    """The ``WPDT`` block: a weapon's numbers, type and flags (32 bytes)."""

    weight: float = 0.0
    value: int = 0
    weapon_type: WeaponType = WeaponType.ShortBladeOneHand
    health: int = 0
    speed: float = 0.0
    reach: float = 0.0
    enchantment: int = 0
    chop_min: int = 0
    chop_max: int = 0
    slash_min: int = 0
    slash_max: int = 0
    thrust_min: int = 0
    thrust_max: int = 0
    flags: WeaponFlags = field(default_factory=lambda: WeaponFlags(0))


@register
@dataclass
class Weapon(Record):
    """A ``WEAP`` record."""

    TAG: ClassVar[bytes] = b"WEAP"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    enchanting: str = ""
    data: WeaponData = field(default_factory=WeaponData)
