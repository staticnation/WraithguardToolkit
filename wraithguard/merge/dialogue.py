"""Dialogue grouping and ``INFO`` ordering, ported from merge_to_master.

A port of ``merge_to_master``'s ``types/dialogue.rs`` (public domain, Greatness7).

A topic (``DIAL``) owns a run of responses (``INFO``) that form a linked list
through each response's ``prev_id``/``next_id``. Merging two plugins' responses
has to preserve that order: a later plugin can retext a response in place, move
one, insert a new one after a named predecessor, or delete one, and the merged
run must read back in the same sequence the game would walk.

:class:`InfoIndex` is the ordering helper. Plugins usually store responses in a
run that already follows the linked-list order, so the response a new one names
as its predecessor is typically the one inserted last -- ``last`` records that
hint, and it is always verified before use. A set of the ids present lets a
brand-new id skip the search entirely.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable

    from wraithguard.esp.records.dialogue import Dialogue
    from wraithguard.esp.records.dialogueinfo import DialogueInfo


class InfoIndex:
    """Speeds up :meth:`DialogueGroup.insert_info` lookups within one group.

    Holds the set of response ids present and a ``last`` position hint. The hint
    is only a guess -- it is checked before it is trusted -- so a wrong hint costs
    a linear search, never a wrong answer.
    """

    def __init__(self, infos: Iterable[DialogueInfo] | None = None) -> None:
        """Build an index, optionally initialised for an existing response run."""
        self._ids: set[str] = set()
        self.last: int = 0
        if infos is not None:
            self.reset(infos)

    def reset(self, infos: Iterable[DialogueInfo]) -> None:
        """Re-initialise for a different group, reusing the allocation."""
        self._ids.clear()
        count = 0
        for info in infos:
            self._ids.add(info.id)
            count += 1
        self.last = count - 1 if count else 0

    def contains(self, info_id: str) -> bool:
        """Whether ``info_id`` is present in the group."""
        return info_id in self._ids

    def insert(self, info_id: str) -> bool:
        """Add ``info_id``; return ``True`` if it was not already present."""
        if info_id in self._ids:
            return False
        self._ids.add(info_id)
        return True

    @staticmethod
    def find(infos: list[DialogueInfo], info_id: str, hint: int) -> int | None:
        """Position of ``info_id``, checking ``hint`` first then searching from the end.

        Args:
            infos: The response run.
            info_id: The id to locate.
            hint: A position to check before scanning.

        Returns:
            The index, or ``None`` if the id is absent.
        """
        if 0 <= hint < len(infos) and infos[hint].id == info_id:
            return hint
        for i in range(len(infos) - 1, -1, -1):
            if infos[i].id == info_id:
                return i
        return None


@dataclass
class DialogueGroup:
    """A topic and its ordered responses -- ``DIAL`` plus the ``INFO`` run under it."""

    dialogue: Dialogue
    infos: list[DialogueInfo] = field(default_factory=list)

    def insert_info(self, info: DialogueInfo, index: InfoIndex) -> None:
        """Insert one response, keeping the linked-list order the file implies.

        If a response with the same id already exists it is replaced (in place
        when only its text changed, else removed and reinserted in the new spot).
        A response with no ``prev_id`` goes to the front; one whose ``prev_id`` is
        present goes just after it; otherwise it goes to the back.

        Args:
            info: The response to insert.
            index: The ordering index for this group; must have been built from
                (and only used with) this group's responses.
        """
        # Does a response with this id already exist? New ids need no search.
        if not index.insert(info.id):
            existing = InfoIndex.find(self.infos, info.id, index.last + 1)
            if existing is not None:
                # If the predecessor is already correct, update in place: the text
                # changed but the ordering did not.
                if self.infos[existing].prev_id == info.prev_id:
                    self.infos[existing] = info
                    index.last = existing
                    return
                # Ordering changed: drop the old entry so it can be reinserted.
                self.infos.pop(existing)

        # No predecessor named: insert at the front.
        if not info.prev_id:
            self.infos.insert(0, info)
            index.last = 0
            return

        # Predecessor named and present: insert just after it.
        if index.contains(info.prev_id):
            after = InfoIndex.find(self.infos, info.prev_id, index.last)
            if after is not None:
                self.infos.insert(after + 1, info)
                index.last = after + 1
                return

        # Predecessor named but not found: insert at the end.
        self.infos.append(info)
        index.last = len(self.infos) - 1

    def merge_infos(self, incoming: Iterable[DialogueInfo]) -> None:
        """Batch-insert responses, reusing one index across the whole run."""
        index = InfoIndex(self.infos)
        for info in incoming:
            self.insert_info(info, index)

    def repair_links(self) -> None:
        """Rewrite ``prev_id``/``next_id`` so adjacent responses point at each other.

        The front's ``prev_id`` and the back's ``next_id`` are left untouched, to
        match the engine's behaviour.
        """
        for i in range(len(self.infos) - 1):
            prev, curr = self.infos[i], self.infos[i + 1]
            if prev.next_id != curr.id:
                prev.next_id = curr.id
            if curr.prev_id != prev.id:
                curr.prev_id = prev.id
