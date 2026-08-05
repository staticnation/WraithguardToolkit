"""Turn a field diff into per-field, per-record and per-plugin conflict status.

The scanner already answers "do these plugins disagree about this field?" with
a yes or a no. That is the wrong resolution for the decision a user is making.
Five plugins editing one record with no loss at all looks exactly like five
plugins where the third one's work is being thrown away, and the second is the
only one worth opening.

This sits between the diff and the display: it takes the values the field diff
produced, applies :mod:`wraithguard.patch.status` to each field, and rolls the
answers up a record at a time and then a plugin at a time. It holds no widgets,
because the roll-up is where the interesting mistakes live and logic behind a
``tkinter`` import cannot be tested without a display.

The plugin roll-up is the part with no equivalent in the current viewer. A flat
list of conflicts cannot answer "which of my mods is losing work?", which is
the question that decides load order.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from wraithguard.patch.status import (
    ABSENT,
    ConflictAll,
    ConflictThis,
    conflict_all,
    conflict_this,
    worst_all,
    worst_this,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable, Mapping, Sequence

    #: Reads one conflict and returns the fields to judge with each plugin's
    #: values for them, or ``None`` when the record cannot be compared. Kept
    #: behind the type-checking guard because nothing needs it at run time.
    ValuesFor = Callable[
        [Mapping[str, Any]], "tuple[Sequence[str], Mapping[str, Mapping[str, Any]]] | None"
    ]


@dataclass(frozen=True, slots=True)
class FieldStatus:
    """What is happening to one field of one record.

    Attributes:
        key: The field's dotted path, as the diff panel labels it.
        overall: The field across every plugin defining the record.
        per_plugin: One status per plugin, in load order.
    """

    key: str
    overall: ConflictAll
    per_plugin: tuple[ConflictThis, ...]


@dataclass(slots=True)
class PluginTally:
    """How one plugin fares across every record it takes part in.

    Attributes:
        plugin: The file name.
        counts: How many records gave it each status.
    """

    plugin: str
    counts: dict[ConflictThis, int] = field(default_factory=dict)

    @property
    def losing(self) -> int:
        """How many records this plugin edits and does not get its way on.

        Returns:
            The count of :attr:`~ConflictThis.CONFLICT_LOSES`.
        """
        return self.counts.get(ConflictThis.CONFLICT_LOSES, 0)

    @property
    def redundant(self) -> int:
        """How many records it redefines without changing anything.

        Not necessarily junk: a dialogue response copied unchanged is often
        there to hold a position. See
        :func:`~wraithguard.patch.records.position_anchors`.

        Returns:
            The count of :attr:`~ConflictThis.IDENTICAL_TO_MASTER`.
        """
        return self.counts.get(ConflictThis.IDENTICAL_TO_MASTER, 0)


def _column(per: Mapping[str, Mapping[str, Any]], plugins: Sequence[str], key: str) -> list[Any]:
    """One field's value from each plugin, in load order.

    Args:
        per: Plugin name to that plugin's field values.
        plugins: The plugins, in load order.
        key: The field.

    Returns:
        One entry per plugin, :data:`~wraithguard.patch.status.ABSENT` where
        the plugin's record does not carry the field at all. Read with ``in``
        rather than ``get``, because ``None`` is a value a record may hold and
        conflating it with absence would call a real edit a no-op.
    """
    return [
        values[key] if key in (values := per.get(plugin) or {}) else ABSENT for plugin in plugins
    ]


def field_statuses(
    keys: Iterable[str],
    per: Mapping[str, Mapping[str, Any]],
    plugins: Sequence[str],
    skip_absent: bool = False,
) -> list[FieldStatus]:
    """Judge every field of one record.

    Args:
        keys: The fields to judge, in display order.
        per: Plugin name to that plugin's field values.
        plugins: The plugins defining the record, **in load order**.
        skip_absent: Treat a missing field as "this plugin has nothing to say"
            rather than as a deletion. Right for optional subrecords.

    Returns:
        One :class:`FieldStatus` per key, in the order given.
    """
    out: list[FieldStatus] = []
    for key in keys:
        values = _column(per, plugins, key)
        out.append(
            FieldStatus(
                key=key,
                overall=conflict_all(values, skip_absent),
                per_plugin=tuple(conflict_this(values, skip_absent)),
            )
        )
    return out


def record_status(statuses: Sequence[FieldStatus]) -> ConflictAll:
    """Roll a record up to its worst field.

    Args:
        statuses: From :func:`field_statuses`.

    Returns:
        The record-wide status.
    """
    return worst_all(status.overall for status in statuses)


def record_plugin_statuses(
    statuses: Sequence[FieldStatus], plugins: Sequence[str]
) -> dict[str, ConflictThis]:
    """Roll a record up to one status per plugin.

    Args:
        statuses: From :func:`field_statuses`.
        plugins: The plugins, in the same order used to build ``statuses``.

    Returns:
        Plugin name to its worst status anywhere in the record.
    """
    return {
        plugin: worst_this(
            status.per_plugin[index] for status in statuses if index < len(status.per_plugin)
        )
        for index, plugin in enumerate(plugins)
    }


def tally(records: Iterable[Mapping[str, ConflictThis]]) -> dict[str, PluginTally]:
    """Count, per plugin, how it fares across many records.

    This is the plugin-level view: not "here are 4,000 conflicts" but "this mod
    loses 38 records and restates 210 unchanged", which is the shape of an
    answer someone can act on.

    Args:
        records: One mapping per record, from :func:`record_plugin_statuses`.

    Returns:
        Plugin name to its tally, in first-seen order.
    """
    out: dict[str, PluginTally] = {}
    for record in records:
        for plugin, status in record.items():
            entry = out.setdefault(plugin, PluginTally(plugin=plugin))
            entry.counts[status] = entry.counts.get(status, 0) + 1
    return out


@dataclass(slots=True)
class Survey:
    """Every conflict judged, at record level and at plugin level.

    Attributes:
        records: ``(type, key)`` to that record's worst status.
        plugins: Plugin name to its tally, worst-status-per-record counted.
        unreadable: How many records could not be compared. Reported rather
            than skipped: a summary that quietly leaves records out is worse
            than one that says how much it could not see.
    """

    records: dict[tuple[str, str], ConflictAll] = field(default_factory=dict)
    plugins: dict[str, PluginTally] = field(default_factory=dict)
    unreadable: int = 0

    @property
    def losing_plugins(self) -> list[PluginTally]:
        """The plugins with work being discarded, worst first.

        Returns:
            Tallies with at least one loss, ordered by how many.
        """
        return sorted(
            (entry for entry in self.plugins.values() if entry.losing),
            key=lambda entry: (-entry.losing, entry.plugin.lower()),
        )


def survey(
    conflicts: Iterable[Mapping[str, Any]],
    values_for: ValuesFor,
    skip_absent: bool = False,
) -> Survey:
    """Judge a whole scan, one record at a time, and tally it per plugin.

    Args:
        conflicts: The conflicts as the scanner reports them. Each needs a
            ``type``, an ``id`` and its ``plugins`` in load order.
        values_for: Called with one conflict; returns ``(keys, per)`` -- the
            fields to judge and each plugin's values -- or ``None`` when the
            record cannot be compared. Passed in rather than done here because
            reading it means running tes3conv, which this module has no
            business knowing about.
        skip_absent: As :func:`field_statuses`.

    Returns:
        The whole scan, judged.
    """
    found = Survey()
    per_record: list[Mapping[str, ConflictThis]] = []

    for conflict in conflicts:
        plugins = list(conflict.get("plugins") or [])
        marker = (str(conflict.get("type") or ""), str(conflict.get("id") or ""))
        read = values_for(conflict)
        if read is None:
            found.unreadable += 1
            continue
        keys, per = read
        statuses = field_statuses(keys, per, plugins, skip_absent)
        found.records[marker] = record_status(statuses)
        per_record.append(record_plugin_statuses(statuses, plugins))

    found.plugins = tally(per_record)
    return found


def row_tag_updates(
    conflicts: Sequence[Mapping[str, Any]],
    records: Mapping[tuple[str, str], ConflictAll],
) -> list[tuple[int, ConflictAll, bool]]:
    """Which listed row takes which judgement, ready for the view to colour.

    The pure core of the conflict list's recolour: given the rows on show and a
    survey's per-record verdicts, say what each row should become. Kept here,
    away from any ``tkinter`` import, so the mapping can be tested off a display
    while the view does nothing but apply these in paced chunks.

    A conflict the survey did not judge (absent from ``records``) is left out,
    so an unjudged row keeps whatever it had rather than being cleared.

    Args:
        conflicts: The rows currently listed, in display order. A row's position
            is its identity in the list, matching how the view fills it.
        records: ``(type, id)`` to that record's status, from a :class:`Survey`.

    Returns:
        ``(index, status, involves_subset)`` for every listed row the survey
        judged, in display order.
    """
    out: list[tuple[int, ConflictAll, bool]] = []
    for index, conflict in enumerate(conflicts):
        marker = (str(conflict.get("type") or ""), str(conflict.get("id") or ""))
        status = records.get(marker)
        if status is None:
            continue
        out.append((index, status, bool(conflict.get("involves_subset"))))
    return out


@dataclass(slots=True)
class Branch:
    """One plugin's contribution to the scan, grouped by record type.

    Attributes:
        plugin: The file name.
        groups: Record type to the ``(type, key)`` markers it defines, in the
            order the scan reported them.
    """

    plugin: str
    groups: dict[str, list[tuple[str, str]]] = field(default_factory=dict)

    @property
    def records(self) -> int:
        """How many records this plugin takes part in.

        Returns:
            The total across every group.
        """
        return sum(len(found) for found in self.groups.values())


def group_by_plugin(
    conflicts: Iterable[Mapping[str, Any]],
    order: Sequence[str] = (),
) -> list[Branch]:
    """Turn a flat conflict list into plugin -> record type -> records.

    This is the shape a load order actually has, and the shape xEdit-style
    editors present: a file, the kinds of thing it changes, and the things
    themselves. The flat list answers "what conflicts"; this answers "what does
    *this mod* touch", which is the question asked while deciding what to do
    about a mod.

    Args:
        conflicts: The conflicts as the scanner reports them.
        order: The load order, which decides the branch order. Plugins not in
            it follow, in first-seen order, rather than being dropped -- a scan
            can outlive the order it was taken from.

    Returns:
        One :class:`Branch` per plugin.
    """
    branches: dict[str, Branch] = {}
    for conflict in conflicts:
        marker = (str(conflict.get("type") or ""), str(conflict.get("id") or ""))
        for plugin in conflict.get("plugins") or []:
            name = str(plugin)
            branch = branches.setdefault(name, Branch(plugin=name))
            branch.groups.setdefault(marker[0], []).append(marker)

    ranked = {name.lower(): position for position, name in enumerate(order)}
    fallback = len(ranked)
    return sorted(
        branches.values(),
        key=lambda branch: (ranked.get(branch.plugin.lower(), fallback), branch.plugin.lower()),
    )


#: How a record-wide status is tagged in a tree, and what it means. Kept beside
#: the model rather than in the window so the two cannot drift, and so the
#: wording can be checked by a test that needs no display.
ALL_TAGS: dict[ConflictAll, tuple[str, str]] = {
    ConflictAll.UNKNOWN: ("status-unknown", "not compared"),
    ConflictAll.ONLY_ONE: ("status-only-one", "only one plugin defines this"),
    ConflictAll.NO_CONFLICT: ("status-agree", "several define it and all agree"),
    ConflictAll.OVERRIDE_BENIGN: (
        "status-benign",
        "overridden, but nothing is lost: every version matches either the "
        "original or the winner",
    ),
    ConflictAll.CONFLICT: (
        "status-conflict",
        "a plugin's edit is being discarded: its value is neither the "
        "original nor the one that wins",
    ),
}
