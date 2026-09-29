"""The record base class and registry.

Every record type is a dataclass here (``wraithguard.esp.records``): the objects the
toolkit reads, compares, merges and writes. Reading and writing the bytes is
greatness7's ``tes3::esp`` (see :mod:`wraithguard.esp.plugin`); the records cross
as tes3conv-schema JSON through :mod:`wraithguard.esp.json`, which finds each class
by name in :data:`REGISTRY`. A record the crate does not model is an
:class:`UnknownRecord` holding its bytes, so a plugin still round-trips whole.
"""

from __future__ import annotations

from abc import ABC
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags

#: Tag (four bytes) -> the record class for it. Populated by
#: :func:`register`, which each record module applies to its class.
REGISTRY: dict[bytes, type[Record]] = {}


class Record(ABC):
    """One TES3 record: its four-byte tag, its object flags and its fields.

    Each subclass is a dataclass of the record's fields.
    """

    #: The record's four-byte tag, e.g. ``b"WEAP"``.
    TAG: ClassVar[bytes]

    #: The record's object flags, read from the header.
    flags: ObjectFlags

    @property
    def wire_tag(self) -> bytes:
        """The tag to write for this record (its class :attr:`TAG`)."""
        return self.TAG


def register(cls: type[Record]) -> type[Record]:
    """Register a record class under its :attr:`~Record.TAG`. Use as a decorator."""
    REGISTRY[cls.TAG] = cls
    return cls


class UnknownRecord(Record):
    """A record type the crate does not model, kept as raw bytes.

    OpenMW's ``LUAL`` and the like: the body (the subrecords after the flags word)
    is written back unchanged.
    """

    TAG = b"\x00\x00\x00\x00"

    def __init__(self, tag: bytes, flags: ObjectFlags, body: bytes) -> None:
        """Hold ``body`` (raw subrecord bytes) under ``tag`` with ``flags``."""
        self._tag = tag
        self.flags = flags
        self.body = body

    @property
    def wire_tag(self) -> bytes:
        """The original tag this record was read under."""
        return self._tag

    def __repr__(self) -> str:
        """A short, tag-and-size summary."""
        return f"UnknownRecord({self._tag!r}, {len(self.body)} bytes)"
