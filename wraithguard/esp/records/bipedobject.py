"""The ``INDX``/``BNAM``/``CNAM`` biped-object group -- a port of ``bipedobject.rs``.

An armor or clothing item covers one or more body slots; each slot is this
three-subrecord group: an ``INDX`` naming the slot (head, cuirass, left glove,
...), then up to two body-part meshes, a male one under ``BNAM`` and a female one
under ``CNAM``. Not a record of its own -- it is a repeated group inside armor
and clothing.

The read looks ahead: after the slot index it *tries* ``BNAM`` then ``CNAM``,
backing out of either tag that is not there, because a slot may have one mesh,
the other, both, or neither. That is why the reader has :meth:`~Reader.try_tag`.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from wraithguard.esp.enums import BipedObjectType

_INDX_SIZE = 1


@dataclass
class BipedObject:
    """One body slot and its male/female meshes (``INDX`` + optional ``BNAM``/``CNAM``)."""

    biped_object_type: BipedObjectType = field(default_factory=BipedObjectType.default)
    male_bodypart: str = ""
    female_bodypart: str = ""
