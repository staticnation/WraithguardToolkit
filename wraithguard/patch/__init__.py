"""Build a patch plugin from records chosen in the diff viewer.

**Everything here is additive.** No source plugin is ever opened for writing,
renamed, or altered in any way. The output is one new file that loads last and
wins by the engine's own rule -- last definition of a record is the one used.
Deleting that file restores the previous behaviour exactly, which is the whole
reason to work this way rather than editing mods in place.

A patch carries *whole records*, not differences. TES3 has no notion of a
partial record: whichever file defines one last supplies all of it. So making
a chosen plugin's version of a record win means carrying that record verbatim,
and everything the patch does not carry still comes from the original mods.

:mod:`wraithguard.patch.records` selects and prepares whole records.
:mod:`wraithguard.patch.merge` builds one record out of several, field by
field, for when neither side of a conflict is right on its own.
:mod:`wraithguard.land.emit` turns them into a plugin document, and tes3conv
writes it -- the same path the merged landscape plugin already takes, because a
plugin is a plugin whatever its records are.
"""

from __future__ import annotations

from wraithguard.patch.align import Row, align, alignable_fields, identity, label_for
from wraithguard.patch.dialogue import (
    Placed,
    Response,
    moved,
    orphans,
    positions,
    responses_by_topic,
    shifts,
    topic_order,
)
from wraithguard.patch.merge import FieldChoice, Merge, describe, merge_record
from wraithguard.patch.records import (
    GREETING,
    PatchError,
    Selection,
    collect,
    defining_plugins,
    dialogue_position_risk,
    index_map,
    master_names,
    needs_remapping,
    position_anchors,
    record_key,
    remap_references,
    required_masters,
    topic_kind,
)
from wraithguard.patch.status import (
    ABSENT,
    ConflictAll,
    ConflictThis,
    conflict_all,
    conflict_this,
    worst_all,
    worst_this,
)

__all__ = [
    "ABSENT",
    "GREETING",
    "ConflictAll",
    "ConflictThis",
    "FieldChoice",
    "Merge",
    "PatchError",
    "Placed",
    "Response",
    "Row",
    "Selection",
    "align",
    "alignable_fields",
    "collect",
    "conflict_all",
    "conflict_this",
    "defining_plugins",
    "describe",
    "dialogue_position_risk",
    "identity",
    "index_map",
    "label_for",
    "master_names",
    "merge_record",
    "moved",
    "needs_remapping",
    "orphans",
    "position_anchors",
    "positions",
    "record_key",
    "remap_references",
    "required_masters",
    "responses_by_topic",
    "shifts",
    "topic_kind",
    "topic_order",
    "worst_all",
    "worst_this",
]
