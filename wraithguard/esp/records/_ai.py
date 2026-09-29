"""AI data and packages, and travel destinations -- shared by NPCs and creatures.

An actor's behaviour is a small ``AIDT`` block (its fight/flee/alarm thresholds
and the services it offers) plus a list of AI *packages*, one per behaviour it
runs: travel to a point, wander an area, escort or follow a target, or activate
something. Escort and follow packages, and travel destinations, may carry a
trailing cell name in a look-ahead sub-block whose bytes fall *outside* the
package's declared size. NPCs and creatures both use all of this, so it lives
here.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, ClassVar

from wraithguard.esp.flags import ServiceFlags


@dataclass
class AiData:
    """The ``AIDT`` block: greeting distance, fight/flee/alarm, services (12 bytes)."""

    hello: int = 0
    fight: int = 0
    flee: int = 0
    alarm: int = 0
    services: ServiceFlags = field(default_factory=lambda: ServiceFlags(0))


@dataclass
class AiTravelPackage:
    """``AI_T`` -- travel to a world point (16 bytes)."""

    TAG: ClassVar[bytes] = b"AI_T"
    SIZE: ClassVar[int] = 16

    location: tuple[float, float, float] = (0.0, 0.0, 0.0)
    reset: int = 0


@dataclass
class AiWanderPackage:
    """``AI_W`` -- wander within a distance, with idle-animation weights (14 bytes)."""

    TAG: ClassVar[bytes] = b"AI_W"
    SIZE: ClassVar[int] = 14

    distance: int = 0
    duration: int = 0
    game_hour: int = 0
    idles: tuple[int, ...] = (0, 0, 0, 0, 0, 0, 0, 0)
    reset: int = 0


@dataclass
class _TargetPackage:
    """Shared body of escort and follow: a point, a target, and an optional cell."""

    SIZE: ClassVar[int] = 48

    location: tuple[float, float, float] = (0.0, 0.0, 0.0)
    duration: int = 0
    target: str = ""
    reset: int = 0
    cell: str = ""


@dataclass
class AiEscortPackage(_TargetPackage):
    """``AI_E`` -- escort a target to a point."""

    TAG: ClassVar[bytes] = b"AI_E"


@dataclass
class AiFollowPackage(_TargetPackage):
    """``AI_F`` -- follow a target."""

    TAG: ClassVar[bytes] = b"AI_F"


@dataclass
class AiActivatePackage:
    """``AI_A`` -- activate a named target (33 bytes)."""

    TAG: ClassVar[bytes] = b"AI_A"
    SIZE: ClassVar[int] = 33

    target: str = ""
    reset: int = 0


#: An AI package is any one of the five kinds.
AiPackage = (
    AiTravelPackage | AiWanderPackage | AiEscortPackage | AiFollowPackage | AiActivatePackage
)

#: The subrecord tag each package kind is read from (each knows its own size).
_AI_PACKAGES: dict[bytes, Any] = {
    AiTravelPackage.TAG: AiTravelPackage,
    AiWanderPackage.TAG: AiWanderPackage,
    AiEscortPackage.TAG: AiEscortPackage,
    AiFollowPackage.TAG: AiFollowPackage,
    AiActivatePackage.TAG: AiActivatePackage,
}


def is_ai_tag(tag: bytes) -> bool:
    """Whether ``tag`` starts an AI package subrecord."""
    return tag in _AI_PACKAGES


@dataclass
class TravelDestination:
    """A ``DODT`` travel target: a transform and an optional destination cell."""

    SIZE: ClassVar[int] = 24

    translation: tuple[float, float, float] = (0.0, 0.0, 0.0)
    rotation: tuple[float, float, float] = (0.0, 0.0, 0.0)
    cell: str = ""
