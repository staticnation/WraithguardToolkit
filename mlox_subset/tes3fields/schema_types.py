"""The shapes the generated TES3 schema is expressed in.

Hand-written and kept separate from ``schema.py`` on purpose: the generated
module is data and gets overwritten wholesale, while these types carry the
behaviour and the docstrings, and should be reviewed like any other code.

The schema describes the *documented* layout of TES3 records. It is a reading
aid, not a parser: a field whose layout could not be read out of the reference
tables mechanically has no members, and a member whose type is variable-length
has a size of zero. Both mean "this table cannot tell you", and a consumer that
treats either as zero bytes will decode garbage confidently -- which is the one
failure mode worth engineering against here.
"""

from __future__ import annotations

from dataclasses import dataclass, field as dataclass_field

#: Cardinality markers as the reference tables use them.
REQUIRED = "+"
OPTIONAL = "-"
REPEATED = "*"

_CARDINALITY_WORDS = {
    REQUIRED: "required",
    OPTIONAL: "optional",
    REPEATED: "may repeat",
}


@dataclass(frozen=True, slots=True)
class Member:
    """One field inside a struct subrecord.

    Attributes:
        type: The scalar type name, e.g. ``"uint16"``.
        dims: Array extents, e.g. ``(16, 16)``; empty for a scalar.
        name: What the reference calls this member.
        size: Width in bytes, or ``0`` when it is variable or unrecognised.
    """

    type: str
    dims: tuple[int, ...]
    name: str
    size: int

    @property
    def count(self) -> int:
        """How many scalars this member holds.

        Returns:
            The product of the extents, or ``1`` for a scalar.
        """
        total = 1
        for dim in self.dims:
            total *= dim
        return total

    def describe(self, limit: int = 64) -> str:
        """Render the member for display.

        Args:
            limit: Longest name to show in full. The reference occasionally
                writes a paragraph where a name belongs (``VHGT``'s height
                array carries its whole explanation), and a layout listing is
                unreadable when one line runs to three hundred characters. The
                full text stays in :attr:`name` for anything that wants it.

        Returns:
            e.g. ``"Level (uint16, 2 bytes)"``.
        """
        extents = "".join(f"[{d}]" for d in self.dims)
        size = f", {self.size} bytes" if self.size else ""
        name = self.name if len(self.name) <= limit else self.name[: limit - 1].rstrip() + "\u2026"
        return f"{name} ({self.type}{extents}{size})"


@dataclass(frozen=True, slots=True)
class Subrecord:
    """One subrecord within a record type.

    Attributes:
        name: The four-character subrecord tag, e.g. ``"NPDT"``.
        cardinality: ``"+"`` required, ``"-"`` optional, ``"*"`` may repeat, or
            empty when the table did not say.
        type: The declared type, e.g. ``"struct"`` or ``"zstring"``.
        size: The declared size as written, e.g. ``"12 or 52 bytes"``. Free text
            because the reference states it as free text, and inventing a number
            where the source hedged would be worse than passing the hedge along.
        description: The first line of the reference's explanation.
        members: The struct layout, when one could be read.
        repeat: How many times the layout repeats to fill the declared size.
            Usually ``1``; ``LAND``'s vertex normals document one three-byte
            element and then declare 12,675 bytes, which is that element 4,225
            times. Recording the multiple keeps the element layout honest
            instead of pretending the struct has 12,675 members.
        variants: Headings for fields documented with more than one layout. A
            field with variants deliberately carries no members: which one
            applies depends on a flag elsewhere in the record, and running the
            two together would mis-read every value after the first.
    """

    name: str
    cardinality: str = ""
    type: str = ""
    size: str = ""
    description: str = ""
    members: tuple[Member, ...] = ()
    repeat: int = 1
    variants: tuple[str, ...] = ()

    @property
    def required(self) -> bool:
        """Whether the reference marks this subrecord as required."""
        return self.cardinality == REQUIRED

    @property
    def repeatable(self) -> bool:
        """Whether the subrecord may appear more than once."""
        return self.cardinality == REPEATED

    @property
    def element_size(self) -> int:
        """The width of one instance of the parsed layout.

        Returns:
            The sum of the members' sizes, or ``0`` if there are none or any is
            variable -- in which case the layout cannot be walked by offset.
        """
        if not self.members or any(m.size == 0 for m in self.members):
            return 0
        return sum(m.size for m in self.members)

    @property
    def fixed_size(self) -> int:
        """The total width the parsed layout accounts for.

        Returns:
            :attr:`element_size` times :attr:`repeat`, which for a repeating
            layout is the whole subrecord and for an ordinary one is the same
            thing. ``0`` when the layout is unknown.
        """
        return self.element_size * self.repeat

    def describe(self) -> str:
        """Render a one-line summary for a tooltip or a header.

        Returns:
            e.g. ``"NPDT - NPC data (struct, 12 or 52 bytes, required)"``.
        """
        parts = [self.type or "?"]
        if self.size:
            parts.append(self.size)
        word = _CARDINALITY_WORDS.get(self.cardinality)
        if word:
            parts.append(word)
        detail = ", ".join(parts)
        head = f"{self.name} - {self.description}" if self.description else self.name
        return f"{head} ({detail})"


@dataclass(frozen=True, slots=True)
class Record:
    """One record type's documented layout.

    Attributes:
        name: The record type, e.g. ``"NPC_"``, or the name of a shared subpage
            such as ``"AI Package Fields"`` whose subrecords several record
            types include.
        description: The reference's summary of what the record holds.
        fields: Its subrecords, in documented order.
    """

    name: str
    description: str = ""
    fields: tuple[Subrecord, ...] = dataclass_field(default_factory=tuple)

    @property
    def by_name(self) -> dict[str, Subrecord]:
        """Subrecords keyed by tag.

        Returns:
            Tag to subrecord. Where a tag is documented twice in one record --
            which happens, as with the ``CNDT`` that follows several different
            AI packages -- the first wins, since that is the one whose
            description applies generally.
        """
        found: dict[str, Subrecord] = {}
        for sub in self.fields:
            found.setdefault(sub.name, sub)
        return found
