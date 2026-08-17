"""Rebuild the order the engine will actually read a topic in.

**Why this is worth doing rather than warning about.** Everything else in this
package can only say that a dialogue response's position *depends on* files it
does not control. That is true and it is nearly useless: it is a warning the
user cannot act on. What they need is the order itself -- this line was fourth,
after your patch it is ninth -- and that can be computed, because the rule the
engine follows is simple and the data to follow it is all present.

**The rule.** Responses are read plugin by plugin in load order. Each carries
``prev_id`` (``PNAM``), naming the response it should follow:

* first time seen, insert it directly after that response;
* ``prev_id`` empty means the top of the topic;
* ``prev_id`` naming something not yet seen means **the end of the topic** --
  the engine has nowhere else to put it;
* seen before, this is an override: update it, and if ``prev_id`` changed,
  move it;
* flagged ``DELETED``, this is a deletion: take the response out of the topic
  (a later plugin re-adding the id inserts it fresh). Removing a response shifts
  everything after it up a place, which -- since position is priority -- is a
  line somebody may start hearing that they did not before. Note the resolver
  does not re-orphan responses that named the deleted line as their
  predecessor; deleting a mid-chain line is an edge rare enough to leave its
  dependents where they were rather than guess.

That third rule is the whole hazard, and it is why an author who inserts a line
in the middle of a topic also drags the neighbouring responses into their
plugin unchanged -- see :func:`~wraithguard.patch.records.position_anchors`.
Break the chain and the line does not vanish; it silently goes last, where its
filters are tested after everything else and may never match.

Ported from ``scanner/dial_info_align.cpp`` in yampt (MIT, Rafał Wierzchoś,
2016-2026).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence


@dataclass(frozen=True, slots=True)
class Response:
    """One definition of a dialogue response, by one plugin.

    Attributes:
        key: The response's id, unique within its topic.
        prev: What it says it follows. Empty means the top of the topic.
        plugin: The file this definition came from.
        deleted: Whether this definition is a deletion (the ``DELETED`` object
            flag). A later plugin deleting a response removes it from the topic.
    """

    key: str
    prev: str = ""
    plugin: str = ""
    deleted: bool = False


@dataclass(slots=True)
class Placed:
    """A response in its resolved position.

    Attributes:
        key: The response's id.
        prev: The ``prev_id`` of the definition that last moved it.
        plugins: Every file defining it, in the order they were read. The last
            is the one the game uses.
    """

    key: str
    prev: str = ""
    plugins: list[str] = field(default_factory=list)


def topic_order(responses: Iterable[Response]) -> list[Placed]:
    """Resolve a topic's responses into the order the engine will read them.

    Args:
        responses: Every definition of every response in one topic, in load
            order. Order is the input; sorting it would change the answer.

    Returns:
        The responses in final order, each naming every plugin that defined it.
    """
    order: list[Placed] = []
    at: dict[str, Placed] = {}

    for response in responses:
        existing = at.get(response.key)
        if response.deleted:
            # A later plugin deleting this response takes it out of the topic.
            # If it was never placed, the deletion is a tombstone with nothing
            # to remove. Dropping it from `at` too means a still-later plugin
            # re-adding the id is treated as a fresh insertion, as the engine
            # would.
            if existing is not None:
                order.remove(existing)
                del at[response.key]
            continue
        if existing is not None:
            if response.plugin:
                existing.plugins.append(response.plugin)
            if existing.prev == response.prev:
                continue
            # An override that moves the line. Take it out and re-place it, so
            # that a plugin relinking a response is followed rather than
            # ignored -- which is exactly what an inserted line does to its
            # neighbours.
            existing.prev = response.prev
            order.remove(existing)
            order.insert(_slot_for(response.prev, order, at), existing)
            continue

        placed = Placed(key=response.key, prev=response.prev)
        if response.plugin:
            placed.plugins.append(response.plugin)
        order.insert(_slot_for(response.prev, order, at), placed)
        at[response.key] = placed

    return order


def _slot_for(prev: str, order: Sequence[Placed], at: Mapping[str, Placed]) -> int:
    """Where a response claiming to follow ``prev`` actually goes.

    Args:
        prev: The ``prev_id`` it declares.
        order: The topic as resolved so far.
        at: Response id to its entry in ``order``.

    Returns:
        An index into ``order``.
    """
    if not prev:
        return 0
    anchor = at.get(prev)
    if anchor is None:
        # The chain is broken: nothing read so far is the response this one
        # claims to follow. The engine cannot place it where it was meant to
        # go, so it lands at the end -- filtered last, and quite possibly
        # never matched.
        return len(order)
    return order.index(anchor) + 1


def positions(order: Sequence[Placed]) -> dict[str, int]:
    """Number the resolved topic, one-based.

    Args:
        order: From :func:`topic_order`.

    Returns:
        Response id to its position.
    """
    return {placed.key: number for number, placed in enumerate(order, start=1)}


def moved(before: Sequence[Placed], after: Sequence[Placed]) -> list[tuple[str, int, int]]:
    """Which responses a change moves, and how far.

    Args:
        before: The topic as resolved without the change.
        after: The topic as resolved with it.

    Returns:
        One ``(response id, old position, new position)`` per response whose
        position changed, ordered by its new position. A response that only
        exists on one side is not reported: appearing is not moving.
    """
    was = positions(before)
    now = positions(after)
    return [
        (key, was[key], place)
        for key, place in sorted(now.items(), key=lambda item: item[1])
        if key in was and was[key] != place
    ]


def responses_by_topic(
    records: Sequence[Mapping[str, object]], plugin: str = ""
) -> dict[str, list[Response]]:
    """Group one plugin's responses under the topics they answer.

    A response carries no topic of its own; it belongs to the last ``Dialogue``
    read before it. Responses before any topic are dropped rather than guessed
    at -- an orphan in the source is the source's problem, and inventing a
    topic for it here would hide it.

    Args:
        records: The plugin's records, in file order.
        plugin: The file name, recorded on each response.

    Returns:
        Topic id to its responses, in file order.
    """
    found: dict[str, list[Response]] = {}
    topic = ""
    for record in records:
        kind = record.get("type")
        if kind == "Dialogue":
            topic = str(record.get("id") or "")
        elif kind == "DialogueInfo" and topic:
            found.setdefault(topic, []).append(
                Response(
                    key=str(record.get("id") or ""),
                    prev=str(record.get("prev_id") or ""),
                    plugin=plugin,
                    deleted="DELETED" in str(record.get("flags") or "").upper(),
                )
            )
    return found


def shifts(
    sources: Mapping[str, Sequence[Mapping[str, object]]],
    load_order: Sequence[str],
    carried: Sequence[Mapping[str, object]],
) -> list[tuple[str, str, int, int]]:
    """Work out which responses a patch moves, and to where.

    Resolves every affected topic twice: once as the load order reads it now,
    and once with the patch appended last. Anything whose number changes is a
    line the patch reorders -- which, since position is priority, means a line
    somebody may stop hearing.

    Args:
        sources: Plugin name to that plugin's records, in file order.
        load_order: The order those plugins load in. Order is the input.
        carried: The records the patch will contain, in order.

    Returns:
        One ``(topic, response id, old position, new position)`` per moved
        response, grouped by topic in load order.
    """
    before: dict[str, list[Response]] = {}
    for plugin in load_order:
        records = sources.get(plugin)
        if records is None:
            continue
        for topic, found in responses_by_topic(records, plugin).items():
            before.setdefault(topic, []).extend(found)

    patched = responses_by_topic(carried, "")
    out: list[tuple[str, str, int, int]] = []
    for topic, added in patched.items():
        was = before.get(topic)
        if not was:
            continue
        out.extend(
            (topic, key, old, new)
            for key, old, new in moved(topic_order(was), topic_order([*was, *added]))
        )
    return out


def orphans(order: Sequence[Placed]) -> list[str]:
    """Responses whose declared predecessor is nowhere in the topic.

    These are the ones the engine pushes to the end. Reporting them is the
    difference between "this may be wrong" and "this line is now last".

    Args:
        order: From :func:`topic_order`.

    Returns:
        Their ids, in resolved order.
    """
    known = {placed.key for placed in order}
    return [placed.key for placed in order if placed.prev and placed.prev not in known]
