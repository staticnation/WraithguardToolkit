"""What a conflict *is*, on two independent axes, from one field to one plugin.

**Why two axes and not one.** A diff viewer that only says "these differ" makes
the reader do the work. The question a modder actually has is two questions at
once, and they have different answers:

* *Is anything being lost here?* -- a property of the record across every file
  that defines it. :class:`ConflictAll`.
* *What is this particular file doing to it?* -- a property of one version.
  :class:`ConflictThis`.

They come apart constantly. Five plugins may all edit one record with no loss
at all, because four of them agree with the last; that is ``OVERRIDE_BENIGN``
across the record while each individual version is ``OVERRIDE_WINS``. Equally,
one plugin can be ``CONFLICT_LOSES`` inside a record that is otherwise fine.
Colouring a tree with a single number cannot say either thing.

**The rule for "nothing is lost".** Given the versions in load order, the first
is the master's and the last is what the game uses. If every version in between
equals one or the other, no author's work is being discarded: each is either
agreeing with what came before or agreeing with what wins. The moment one
version is *neither*, that author's edit is being thrown away silently, and
that is the only case worth alarming about.

**Rolling up.** A group takes the worst of its records and a plugin takes the
worst of its groups, on each axis separately. That is why the enum orders are
meaningful rather than decorative, and why :data:`THIS_SEVERITY` exists: the
declaration order of :class:`ConflictThis` follows xEdit's naming, which is not
the order of how much a user should care.

Ported from ``scanner/record_conflict.cpp`` and the roll-up in
``model/nav_tree_model.cpp`` of yampt (MIT, Rafał Wierzchoś, 2016-2026), whose
naming in turn follows TES5Edit/xEdit's "Conflict Status All" and "Conflict
Status This". The algorithm is theirs; the sentinel handling below is ours,
because Python can tell absence from emptiness and C++ strings there could not.
"""

from __future__ import annotations

import enum
from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence


class _Absent:
    """A field this version of the record does not have at all.

    Distinct from ``None`` and from ``""``, both of which are values a record
    may legitimately carry. yampt needed a magic byte string for this
    (``non_existent_value``); a singleton says the same thing without the
    chance of a real value colliding with it.
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """Name it as it is written, so failures read clearly."""
        return "ABSENT"


#: The field is not present in this version of the record.
ABSENT: Final = _Absent()


class ConflictAll(enum.IntEnum):
    """What is happening to a record across every file that defines it.

    Ordered by how much attention it deserves, so rolling a group or a plugin
    up to its worst member is a :func:`max`.

    Attributes:
        UNKNOWN: Nothing was computed.
        ONLY_ONE: One file defines it. Nothing can be in conflict.
        NO_CONFLICT: Several define it and they all agree.
        OVERRIDE_BENIGN: Several disagree, but every version equals either the
            master's or the winner's, so no edit is being discarded.
        CONFLICT: Some version is neither, so that author's change is lost.
    """

    UNKNOWN = 0
    ONLY_ONE = 1
    NO_CONFLICT = 2
    OVERRIDE_BENIGN = 3
    CONFLICT = 4


class ConflictThis(enum.Enum):
    """What one file is doing to a record the others also touch.

    Not ordered by severity -- the names follow xEdit, whose order is
    historical. Use :data:`THIS_SEVERITY` to compare two of these.

    Attributes:
        UNKNOWN: Not computed, or this file does not define it.
        IGNORED: Excluded from the comparison by policy.
        MASTER: The first definition. Everything else overrides this.
        IDENTICAL_TO_MASTER: Defined again with no change -- the "dirty" or
            ITM record. Usually removable, but not always: a dialogue response
            copied unchanged may be there to hold a position.
        OVERRIDE_WINS: Changed it, and nothing later disagrees.
        CONFLICT_WINS: Changed it, others disagreed, and this one is used.
        CONFLICT_LOSES: Changed it and something later overrode the change.
            This is the status worth hunting for; it is work being thrown away.
        DELETED: Marked deleted.
    """

    UNKNOWN = "unknown"
    IGNORED = "ignored"
    MASTER = "master"
    IDENTICAL_TO_MASTER = "identical to master"
    OVERRIDE_WINS = "override, wins"
    CONFLICT_WINS = "conflict, wins"
    CONFLICT_LOSES = "conflict, loses"
    DELETED = "deleted"


#: How much each :class:`ConflictThis` matters, for rolling up to the worst.
#: ``CONFLICT_LOSES`` outranks ``CONFLICT_WINS`` deliberately: a file whose
#: edit survives needs no attention, and a file whose edit vanished does.
THIS_SEVERITY: Final[dict[ConflictThis, int]] = {
    ConflictThis.UNKNOWN: 0,
    ConflictThis.DELETED: 0,
    ConflictThis.IGNORED: 0,
    ConflictThis.IDENTICAL_TO_MASTER: 1,
    ConflictThis.MASTER: 2,
    ConflictThis.OVERRIDE_WINS: 3,
    ConflictThis.CONFLICT_WINS: 4,
    ConflictThis.CONFLICT_LOSES: 5,
}


def _present(values: Sequence[Any], skip_absent: bool) -> list[int]:
    """The positions that take part in the comparison.

    Args:
        values: One value per file, in load order.
        skip_absent: Leave :data:`ABSENT` entries out entirely, rather than
            treating them as a value that happens to be missing.

    Returns:
        Indices into ``values``, in order.
    """
    if not skip_absent:
        return list(range(len(values)))
    return [index for index, value in enumerate(values) if value is not ABSENT]


def _nothing_is_lost(values: Sequence[Any], taking_part: Sequence[int]) -> bool:
    """Whether every version agrees with either the master or the winner.

    This is the whole judgement. If it holds, the disagreements are all either
    "same as before" or "same as what wins", and no author's edit is being
    discarded. If it fails, at least one file's change is not reaching the
    game and nothing else will tell the user so.

    Args:
        values: One value per file, in load order.
        taking_part: The positions that count, from :func:`_present`.

    Returns:
        ``True`` when no edit is being discarded.
    """
    first = values[taking_part[0]]
    winner = values[taking_part[-1]]
    return not any(
        values[index] != first and values[index] != winner
        for index in taking_part
        if values[index] is not ABSENT
    )


def conflict_all(values: Sequence[Any], skip_absent: bool = False) -> ConflictAll:
    """Judge a record or field across every file that defines it.

    Args:
        values: One value per file, **in load order** -- the first is the
            master's and the last is the one the game uses. Order is the whole
            input; sorting it would change the answer.
        skip_absent: Treat :data:`ABSENT` as "this file has nothing to say"
            rather than as a value. Right for optional subrecords, wrong for a
            field whose absence is itself the edit.

    Returns:
        The record-wide status.
    """
    taking_part = _present(values, skip_absent)
    if len(taking_part) <= 1:
        return ConflictAll.ONLY_ONE

    first = values[taking_part[0]]
    if all(values[index] == first for index in taking_part[1:]):
        return ConflictAll.NO_CONFLICT

    return (
        ConflictAll.OVERRIDE_BENIGN
        if _nothing_is_lost(values, taking_part)
        else ConflictAll.CONFLICT
    )


def conflict_this(values: Sequence[Any], skip_absent: bool = False) -> list[ConflictThis]:
    """Judge what each individual file is doing.

    Args:
        values: One value per file, in load order.
        skip_absent: As :func:`conflict_all`.

    Returns:
        One status per entry of ``values``, in the same order.
    """
    result = [ConflictThis.UNKNOWN] * len(values)
    taking_part = _present(values, skip_absent)
    if not taking_part:
        return result

    first_at = taking_part[0]
    last_at = taking_part[-1]
    # A file that does not have the field is not the authority on it, even when
    # it is first. yampt makes the same call, and it matters for the skip_absent
    # case where "first" only means "first to say anything".
    result[first_at] = ConflictThis.UNKNOWN if values[first_at] is ABSENT else ConflictThis.MASTER
    if len(taking_part) == 1:
        return result

    first = values[first_at]
    benign = _nothing_is_lost(values, taking_part)

    for index in taking_part[1:]:
        value = values[index]
        if value is ABSENT:
            # Only reachable when skip_absent is False, where absence is a
            # value: absent-where-the-master-was-absent is no change at all.
            result[index] = (
                ConflictThis.IDENTICAL_TO_MASTER if first is ABSENT else ConflictThis.UNKNOWN
            )
        elif value == first:
            result[index] = ConflictThis.IDENTICAL_TO_MASTER
        elif benign:
            result[index] = ConflictThis.OVERRIDE_WINS
        elif index == last_at:
            result[index] = ConflictThis.CONFLICT_WINS
        else:
            result[index] = ConflictThis.CONFLICT_LOSES

    return result


def worst_all(statuses: Iterable[ConflictAll]) -> ConflictAll:
    """Roll a group or a plugin up to its worst record.

    Args:
        statuses: The record-wide statuses beneath it.

    Returns:
        The most severe, or :attr:`ConflictAll.ONLY_ONE` when there are none.
    """
    return max(statuses, default=ConflictAll.ONLY_ONE)


def worst_this(statuses: Iterable[ConflictThis]) -> ConflictThis:
    """Roll a group or a plugin up to its worst version status.

    Args:
        statuses: The per-file statuses beneath it.

    Returns:
        The most severe by :data:`THIS_SEVERITY`, or
        :attr:`ConflictThis.UNKNOWN` when there are none.
    """
    return max(statuses, key=lambda status: THIS_SEVERITY[status], default=ConflictThis.UNKNOWN)
