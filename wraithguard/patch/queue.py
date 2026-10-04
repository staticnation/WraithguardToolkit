"""The decisions queued for a patch, and the rules for changing them.

Queuing a record is a decision, and decisions get revisited: you pick a winner,
look at three more conflicts, then realise the first one should have taken one
field from somewhere else. So the queue has to be editable, and editing it has
rules that are easy to get subtly wrong.

This holds no widgets and imports nothing from the interface. That is
deliberate and was learned the hard way twice in this codebase: logic behind a
``tkinter`` import cannot be tested without a display, and what cannot be
tested is where the bugs live.

**The rules.**

*Re-deciding replaces.* Choosing a winner for a record you already chose, or a
plugin for a field you already picked, overwrites the earlier answer. Keeping
both would put two versions of one record in the patch and leave the patch's
*own* last-wins to decide, so the answer given last might not be the one that
reaches the game.

*Whole or merged, never both.* Same reason. Choosing one drops the other.

*The base is whatever currently wins.* A merge is then a list of departures
from what the load order already does, which is the smallest thing that can be
wrong. The conflict scan says what wins for a record in conflict; for one it does
not list (the viewer's editor edits any record), whoever queued it says, with
:meth:`PatchQueue.set_base`.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wraithguard.patch.merge import Merge

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence

    from wraithguard.patch.merge import Choice
    from wraithguard.patch.records import Selection
    from wraithguard.patch.refedit import NewRef, RefEdit


class PatchQueue:
    """What a patch will carry, before it is written."""

    def __init__(self) -> None:
        """Start empty."""
        self._whole: list[Selection] = []
        self._fields: dict[tuple[str, str], list[Choice]] = {}
        self._bases: dict[tuple[str, str], str] = {}
        self._refs: dict[tuple[str, str, int], RefEdit] = {}
        self._new: dict[tuple[str, str], NewRef] = {}

    @property
    def selections(self) -> list[Selection]:
        """Records to be taken whole, in the order chosen."""
        return self._whole

    @property
    def fields(self) -> dict[tuple[str, str], list[Choice]]:
        """Field decisions (from a plugin or literal), keyed by ``(record type, key)``."""
        return self._fields

    def __len__(self) -> int:
        """How many records the patch would carry (a cell with changed references is one)."""
        records = {(s.record_type, s.key.lower()) for s in self._whole}
        records |= {(t, k.lower()) for t, k in self._fields}
        cells = {("Cell", e.cell.lower()) for e in self._refs.values()}
        cells |= {("Cell", n.cell.lower()) for n in self._new.values()}
        cells -= records
        return len(self._whole) + len(self._fields) + len(cells)

    @property
    def ref_edits(self) -> list[RefEdit]:
        """Changes to placed objects, one per reference, in the order made."""
        return list(self._refs.values())

    def add_ref_edit(self, edit: RefEdit) -> None:
        """Queue changes to a placed object.

        Changes already queued for it are kept, the new ones replacing any to the
        same fields.

        Args:
            edit: The changes.
        """
        from dataclasses import replace

        old = self._refs.get(edit.ident)
        if old is not None:
            edit = replace(
                edit,
                changes={**old.changes, **edit.changes},
                plugins=tuple(dict.fromkeys((*old.plugins, *edit.plugins))),
            )
        self._refs[edit.ident] = edit

    @property
    def new_refs(self) -> list[NewRef]:
        """References the patch adds, in the order placed."""
        return list(self._new.values())

    def add_new_ref(self, new: NewRef) -> None:
        """Queue a reference to add, or change one queued (its fields merged).

        Args:
            new: The reference.
        """
        from dataclasses import replace

        old = self._new.get(new.ident)
        if old is not None:
            new = replace(old, fields={**old.fields, **new.fields})
        self._new[new.ident] = new

    def remove_new_ref(self, cell: str, uid: str) -> None:
        """Drop a reference queued to be added.

        Args:
            cell: Its cell's key.
            uid: Its editor name.
        """
        self._new.pop((cell.lower(), uid), None)

    def remove_ref_edit(
        self, cell: str, origin: str, refr_index: int, path: str | None = None
    ) -> None:
        """Drop the changes to a placed object, or to one of its fields.

        Args:
            cell: The cell's key.
            origin: The plugin that created the reference.
            refr_index: Its index.
            path: One field, or None for all of them.
        """
        from dataclasses import replace

        ident = (cell.lower(), origin.lower(), refr_index)
        old = self._refs.get(ident)
        if old is None:
            return
        if path is None:
            del self._refs[ident]
            return
        rest = {k: v for k, v in old.changes.items() if k != path}
        if rest:
            self._refs[ident] = replace(old, changes=rest)
        else:
            del self._refs[ident]

    def clear(self) -> None:
        """Drop every decision."""
        self._whole.clear()
        self._fields.clear()
        self._bases.clear()
        self._refs.clear()
        self._new.clear()

    def set_base(self, record_type: str, key: str, plugin: str) -> None:
        """Say which plugin wins a record, for one the conflict scan does not list.

        Used by :meth:`merges` when its ``base_for`` has no answer: the viewer's
        editor changes records that conflict with nothing, and knows (from the load
        order it loaded) which plugin defines them last.

        Args:
            record_type: The record's type.
            key: Its identifying key.
            plugin: The plugin that currently wins it.
        """
        self._bases[(record_type, key)] = plugin

    def base(self, record_type: str, key: str) -> str:
        """The base given to :meth:`set_base` for a record, or ``""``.

        Args:
            record_type: The record's type.
            key: Its identifying key.

        Returns:
            The plugin.
        """
        return self._bases.get((record_type, key), "")

    def add_whole(self, selection: Selection) -> None:
        """Queue a record to be taken whole.

        Args:
            selection: The record, and the plugin whose version wins.
        """
        self._drop_whole(selection.record_type, selection.key)
        self._whole.append(selection)
        self._fields.pop((selection.record_type, selection.key), None)

    def add_field(self, record_type: str, key: str, choice: Choice) -> None:
        """Queue one field of a record.

        Args:
            record_type: The record's type.
            key: Its identifying key.
            choice: The field, either taken from a plugin (``FieldChoice``) or
                given a literal value (``FieldValue``). Re-deciding the same
                path replaces the earlier answer, whichever kind either was.
        """
        choices = self._fields.setdefault((record_type, key), [])
        choices[:] = [entry for entry in choices if entry.path != choice.path]
        choices.append(choice)
        self._drop_whole(record_type, key)

    def remove_record(self, record_type: str, key: str) -> None:
        """Drop a record entirely, however it was queued.

        Args:
            record_type: The record's type.
            key: Its identifying key.
        """
        self._drop_whole(record_type, key)
        self._fields.pop((record_type, key), None)
        self._bases.pop((record_type, key), None)

    def remove_field(self, record_type: str, key: str, path: str) -> None:
        """Drop one field choice.

        A record left with no field choices is dropped too: it would write the
        base record unchanged, which is what the load order already does.

        Args:
            record_type: The record's type.
            key: Its identifying key.
            path: The field's dotted path.
        """
        choices = self._fields.get((record_type, key))
        if choices is None:
            return
        choices[:] = [entry for entry in choices if entry.path != path]
        if not choices:
            self._fields.pop((record_type, key), None)
            self._bases.pop((record_type, key), None)

    def merges(self, base_for: Callable[[str, str], str]) -> list[Merge]:
        """The queued field choices, as the writer takes them.

        Args:
            base_for: Called with ``(record_type, key)``; returns the plugin
                supplying the fields not chosen. Passed in rather than looked
                up here because it depends on the current scan, which this does
                not know about. When it answers ``""``, the base given to
                :meth:`set_base` is used.

        Returns:
            One :class:`~wraithguard.patch.merge.Merge` per record.
        """
        return [
            Merge(
                record_type=record_type,
                key=key,
                base_plugin=base_for(record_type, key) or self._bases.get((record_type, key), ""),
                choices=tuple(choices),
            )
            for (record_type, key), choices in self._fields.items()
        ]

    def _drop_whole(self, record_type: str, key: str) -> None:
        """Remove any whole-record choice for one record.

        Args:
            record_type: The record's type.
            key: Its identifying key.
        """
        self._whole[:] = [
            entry
            for entry in self._whole
            if not (entry.record_type == record_type and entry.key == key)
        ]


def base_from_conflicts(conflicts: Sequence[Mapping[str, Any]], record_type: str, key: str) -> str:
    """Find which plugin currently wins a record, from a scan's conflict list.

    Args:
        conflicts: The conflicts as the scanner reports them.
        record_type: The record's type.
        key: Its identifying key.

    Returns:
        The plugin that loads last among those defining it, or an empty string
        when the record is not in the list -- which means the queue has
        outlived a rescan, and is worth reporting rather than guessing at.
    """
    for conflict in conflicts:
        if str(conflict.get("type")) == record_type and str(conflict.get("id")) == key:
            plugins = conflict.get("plugins") or [""]
            return str(plugins[-1])
    return ""
