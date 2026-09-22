"""The in-memory buckets a plugin is merged in, ported from merge_to_master.

A port of ``merge_to_master``'s ``types/plugin.rs`` and ``types/cells.rs``
(public domain, Greatness7).

A flat record list is awkward to merge: you want to find the winning version of
an object by its identity, union a cell's references, and splice a topic's
responses in order. :class:`PluginData` is that view. It sorts a plugin's records
into four buckets -- the header, the ordinary objects keyed by identity, the
cells (interiors by name, exteriors by grid, each holding its cell, landscape and
path grid), and the dialogue groups -- and can re-emit a flat, ordered record
list once the merge is done.

**Object identity.** Most record types are keyed by ``(tag, id)``. The "physical"
objects a cell can place (statics, doors, NPCs, items, ...) instead share one id
space -- a fixed zero tag as the key's tag -- because Morrowind requires their
ids to be unique across all of those types at once, and a reference names one by
id alone.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from wraithguard.esp.enums import DialogueType2
from wraithguard.esp.flags import CellFlags, ObjectFlags
from wraithguard.esp.records.cell import Cell
from wraithguard.esp.records.dialogue import Dialogue
from wraithguard.esp.records.dialogueinfo import DialogueInfo
from wraithguard.esp.records.header import Header
from wraithguard.esp.records.landscape import Landscape
from wraithguard.esp.records.magiceffect import MagicEffect
from wraithguard.esp.records.pathgrid import PathGrid
from wraithguard.esp.records.skill import Skill
from wraithguard.merge.dialogue import DialogueGroup, InfoIndex

if TYPE_CHECKING:
    from collections.abc import Iterable, Iterator

    from wraithguard.esp.record import Record
    from wraithguard.esp.records.reference import Reference

#: The key tag used for every "physical" object, so they share one id space.
ZERO_TAG: bytes = b"\x00\x00\x00\x00"

#: Record tags keyed by ``(tag, id)`` -- each type keeps its own id space.
SHARED_ID_TAGS: frozenset[bytes] = frozenset(
    {
        b"BSGN",
        b"CLAS",
        b"FACT",
        b"GLOB",
        b"LTEX",
        b"MGEF",
        b"RACE",
        b"REGN",
        b"SCPT",
        b"SKIL",
        b"SOUN",
        b"SNDG",
        b"SSCR",
        b"GMST",
    }
)

#: Record tags for the "physical" objects that share one id space (key tag is
#: :data:`ZERO_TAG`). These are the objects a cell reference can name.
PHYSICAL_TAGS: frozenset[bytes] = frozenset(
    {
        b"ACTI",
        b"ALCH",
        b"APPA",
        b"ARMO",
        b"BODY",
        b"BOOK",
        b"CLOT",
        b"CONT",
        b"CREA",
        b"DOOR",
        b"ENCH",
        b"INGR",
        b"LEVC",
        b"LEVI",
        b"LIGH",
        b"LOCK",
        b"MISC",
        b"NPC_",
        b"PROB",
        b"REPA",
        b"SPEL",
        b"STAT",
        b"WEAP",
    }
)

#: An object identity: the key tag (a real tag, or :data:`ZERO_TAG`) and the
#: lower-cased id.
ObjectKey = tuple[bytes, str]


def is_deleted(record: Record) -> bool:
    """Whether a record carries the DELETED object flag."""
    return bool(record.flags & ObjectFlags.DELETED)


def is_ignored(record: Record) -> bool:
    """Whether a record carries the IGNORED object flag."""
    return bool(record.flags & ObjectFlags.IGNORED)


def set_ignored(record: Record, ignored: bool) -> None:
    """Set or clear a record's IGNORED flag."""
    if ignored:
        record.flags |= ObjectFlags.IGNORED
    else:
        record.flags &= ~ObjectFlags.IGNORED


def editor_id(record: Record) -> str:
    """The record's editor id, as the merge keys and reports it.

    Most records carry an ``id``; ``SKIL`` and ``MGEF`` are numbered instead, so
    their fixed index stands in as the id.

    Args:
        record: Any record.

    Returns:
        The id string (possibly empty for a record that has none).
    """
    if isinstance(record, Skill):
        return str(int(record.skill_id))
    if isinstance(record, MagicEffect):
        return str(int(record.effect_id))
    return str(getattr(record, "id", ""))


def object_key(record: Record) -> ObjectKey | None:
    """The identity key for an ordinary object, or ``None`` for a non-object record.

    Cells, landscapes, path grids, dialogue, responses and the header are not
    ordinary objects -- they live in their own buckets -- so this returns
    ``None`` for them.

    Args:
        record: Any record.

    Returns:
        ``(key_tag, id_lower)`` for an ordinary object with a non-empty id, else
        ``None``.
    """
    tag = record.TAG
    if tag in PHYSICAL_TAGS:
        key_tag = ZERO_TAG
    elif tag in SHARED_ID_TAGS:
        key_tag = tag
    else:
        return None
    ident = editor_id(record).lower()
    if not ident:
        return None
    return (key_tag, ident)


def _dedup_references(cell: Cell) -> None:
    """Collapse references sharing a ``(mast_index, refr_index)`` key, keeping the last.

    tes3 stores a cell's references in a map keyed by that pair, so a malformed
    cell that lists the same object twice keeps only its last entry on load. Our
    reader keeps a list, so we reproduce that collapse here to match the engine's
    (and merge_to_master's) view of a cell.

    Args:
        cell: The cell whose reference list is normalised in place.
    """
    merged: dict[tuple[int, int], Reference] = {}
    for reference in cell.references:
        merged[(reference.mast_index, reference.refr_index)] = reference
    if len(merged) != len(cell.references):
        cell.references = list(merged.values())


def exterior_coords(cell: Cell) -> tuple[int, int] | None:
    """A cell's exterior grid coordinates, or ``None`` if it is an interior."""
    if cell.data.cell_flags & CellFlags.IS_INTERIOR:
        return None
    return cell.data.grid


@dataclass
class Interior:
    """An interior cell and its optional path grid."""

    cell: Cell | None = None
    pathgrid: PathGrid | None = None

    def count_objects(self) -> int:
        """How many records this interior would emit."""
        return (self.cell is not None) + (self.pathgrid is not None)


@dataclass
class Exterior:
    """An exterior cell and its optional landscape and path grid."""

    cell: Cell | None = None
    landscape: Landscape | None = None
    pathgrid: PathGrid | None = None

    def count_objects(self) -> int:
        """How many records this exterior would emit."""
        return (self.cell is not None) + (self.landscape is not None) + (self.pathgrid is not None)


@dataclass
class Cells:
    """Every cell in a plugin, split by kind."""

    exteriors: dict[tuple[int, int], Exterior] = field(default_factory=dict)
    interiors: dict[str, Interior] = field(default_factory=dict)

    def get_or_create_exterior(self, coords: tuple[int, int]) -> Exterior:
        """The exterior at ``coords``, created empty if absent."""
        return self.exteriors.setdefault(coords, Exterior())

    def get_or_create_interior(self, name: str) -> Interior:
        """The interior named ``name`` (case-insensitively), created if absent."""
        return self.interiors.setdefault(name.lower(), Interior())

    def get_interior_mut(self, name: str) -> Interior | None:
        """The interior named ``name``, or ``None``."""
        return self.interiors.get(name.lower())

    def get_exterior_mut(self, coords: tuple[int, int]) -> Exterior | None:
        """The exterior at ``coords``, or ``None``."""
        return self.exteriors.get(coords)

    def iter_cells(self) -> Iterator[Cell]:
        """Every present cell, exteriors then interiors."""
        for exterior in self.exteriors.values():
            if exterior.cell is not None:
                yield exterior.cell
        for interior in self.interiors.values():
            if interior.cell is not None:
                yield interior.cell

    def count_objects(self) -> int:
        """How many records all cells would emit."""
        return sum(e.count_objects() for e in self.exteriors.values()) + sum(
            i.count_objects() for i in self.interiors.values()
        )


@dataclass
class PluginData:
    """A plugin sorted into merge buckets: header, objects, cells, dialogue."""

    header: Header = field(default_factory=Header)
    objects: dict[ObjectKey, Record] = field(default_factory=dict)
    cells: Cells = field(default_factory=Cells)
    dialogues: dict[str, DialogueGroup] = field(default_factory=dict)

    @classmethod
    def from_records(cls, records: Iterable[Record]) -> PluginData:
        """Sort a flat record list into a :class:`PluginData`."""
        self = cls()
        self.collect_objects(records)
        return self

    def collect_objects(self, records: Iterable[Record]) -> None:
        """Bucket every record, grouping cells and threading dialogue responses.

        Args:
            records: The plugin's records in file order (the header first).
        """
        from wraithguard.merge.combine import merge_cell_into

        current_topic = ""
        info_index = InfoIndex()

        for record in records:
            if isinstance(record, Header):
                self.header = record
            elif isinstance(record, Cell):
                coords = exterior_coords(record)
                slot: Interior | Exterior
                if coords is not None:
                    slot = self.cells.get_or_create_exterior(coords)
                else:
                    slot = self.cells.get_or_create_interior(record.name)
                if slot.cell is None:
                    _dedup_references(record)
                    slot.cell = record
                else:
                    merge_cell_into(record, slot.cell)
            elif isinstance(record, Landscape):
                self.cells.get_or_create_exterior(record.grid).landscape = record
            elif isinstance(record, PathGrid):
                self._collect_pathgrid(record)
            elif isinstance(record, Dialogue):
                current_topic = record.id.lower()
                group = self.dialogues.get(current_topic)
                if group is None:
                    group = DialogueGroup(dialogue=record)
                    self.dialogues[current_topic] = group
                else:
                    group.dialogue = record
                info_index.reset(group.infos)
            elif isinstance(record, DialogueInfo):
                group = self.dialogues[current_topic]
                group.insert_info(record, info_index)
            else:
                key = object_key(record)
                if key is not None:
                    self.objects[key] = record

    def _collect_pathgrid(self, pathgrid: PathGrid) -> None:
        """Attach a path grid to its cell, handling the orphan cases M2M documents."""
        interior = self.cells.get_interior_mut(pathgrid.cell)
        if interior is not None:
            interior.pathgrid = pathgrid
            return
        exterior = self.cells.get_exterior_mut(pathgrid.data.grid)
        if exterior is not None:
            exterior.pathgrid = pathgrid
            return
        # Orphan: its cell is not in this plugin. An interior path grid defaults to
        # grid (0, 0); a named cell is far more likely an interior defined earlier
        # in the load order than the origin exterior, so assume that.
        if pathgrid.data.grid == (0, 0) and pathgrid.cell:
            self.cells.get_or_create_interior(pathgrid.cell).pathgrid = pathgrid
        else:
            self.cells.get_or_create_exterior(pathgrid.data.grid).pathgrid = pathgrid

    def count_objects(self) -> int:
        """How many records :meth:`into_records` would emit (including the header)."""
        dialogue_count = sum(1 + len(g.infos) for g in self.dialogues.values())
        return 1 + len(self.objects) + self.cells.count_objects() + dialogue_count

    def into_records(self) -> list[Record]:
        """Re-emit a flat, ordered record list: header, objects, cells, dialogue."""
        records: list[Record] = [self.header]
        records.extend(
            self.objects[key] for key in sorted(self.objects, key=lambda k: (k[0], k[1]))
        )
        records.extend(self._cell_records())
        records.extend(self._dialogue_records())
        self.header.num_objects = len(records) - 1
        return records

    def _cell_records(self) -> Iterator[Record]:
        """Cell records in a deterministic order: exteriors by grid, interiors by name."""
        for coords in sorted(self.cells.exteriors):
            ext = self.cells.exteriors[coords]
            if ext.cell is not None:
                yield ext.cell
            if ext.landscape is not None:
                yield ext.landscape
            if ext.pathgrid is not None:
                yield ext.pathgrid
        for name in sorted(self.cells.interiors):
            intr = self.cells.interiors[name]
            if intr.cell is not None:
                yield intr.cell
            if intr.pathgrid is not None:
                yield intr.pathgrid

    def _dialogue_records(self) -> Iterator[Record]:
        """Dialogue records, journals first then topics, each followed by its responses."""

        def priority(group: DialogueGroup) -> int:
            """Sort key placing journals first, as the engine requires."""
            order = {
                DialogueType2.Journal: 0,
                DialogueType2.Topic: 1,
                DialogueType2.Voice: 2,
                DialogueType2.Greeting: 3,
                DialogueType2.Persuasion: 4,
            }
            return order.get(group.dialogue.dialogue_type, 5)

        for group in sorted(
            self.dialogues.values(), key=lambda g: (priority(g), g.dialogue.id.lower())
        ):
            yield group.dialogue
            yield from group.infos
