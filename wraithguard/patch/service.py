"""One call that turns chosen records into a patch plugin.

The GUI and any future command-line caller need the same four steps in the same
order, and neither should own them: read the source plugins, work out what the
patch must declare, carry the records across with their references remapped,
and write one new file.

**Everything it does is additive.** No source plugin is opened for writing. The
output is one new file that loads last; deleting it restores the previous
behaviour exactly. The only destructive act available is overwriting a previous
patch of the same name, and that is the caller's decision.
"""

from __future__ import annotations

import json
import logging
import subprocess
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final

from wraithguard.land.emit import EmitError, build_plugin
from wraithguard.patch.merge import Merge, describe, merge_record
from wraithguard.patch.records import (
    PatchError,
    Selection,
    collect,
    dialogue_position_risk,
    position_anchors,
    required_masters,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence

_log: Final = logging.getLogger(__name__)

#: What the patch is called when the caller does not say.
DEFAULT_NAME: Final = "Wraithguard Patch.esp"

#: What the header says it is, so anyone opening it later knows.
DESCRIPTION: Final = "Record patch built by Wraithguard Toolkit. Load last."


class PatchServiceError(Exception):
    """Raised when a patch cannot be built or written."""


@dataclass(slots=True)
class PatchResult:
    """What a patch build produced.

    Attributes:
        output: The plugin written, or ``None`` for a dry run.
        records: How many records it carries.
        masters: The files it declares, in order.
        remapped: Records whose references had to be renumbered.
        lines: The progress report, for a log panel.
    """

    output: Path | None = None
    records: int = 0
    masters: list[str] = field(default_factory=list)
    remapped: int = 0
    lines: list[str] = field(default_factory=list)


def build_record_patch(
    selections: Sequence[Selection],
    records_by_plugin: Mapping[str, Sequence[Mapping[str, Any]]],
    load_order: Sequence[str],
    sizes: Mapping[str, int],
    converter: str,
    output: Path,
    merges: Sequence[Merge] = (),
    dry_run: bool = False,
    report: Callable[[str], None] | None = None,
) -> PatchResult:
    """Build and write a patch carrying the chosen records.

    Args:
        selections: Which whole record to take from which plugin.
        merges: Records built from several plugins field by field, for when
            neither side of a conflict is right on its own. A record may be in
            ``selections`` or ``merges``, never both -- carrying it twice would
            leave the patch's own last-wins to decide, which is not a choice
            the user made.
        records_by_plugin: Each source plugin's decoded records, including its
            header -- the master list is read from there, and without it the
            references cannot be remapped.
        load_order: The full load order, which decides the order the patch
            declares its masters in. Order *is* meaning here: ``mast_index``
            values are positions in that list.
        sizes: Each master's size in bytes, for the header.
        converter: Path to ``tes3conv``, which does the binary encoding.
        output: Where to write.
        dry_run: Report without writing.
        report: Called with each progress line, if given.

    Returns:
        What was produced.

    Raises:
        PatchServiceError: If the patch cannot be built or written.
    """
    lines: list[str] = []

    def say(text: str) -> None:
        lines.append(text)
        if report is not None:
            report(text)

    if not selections and not merges:
        raise PatchServiceError("nothing was selected, so there is no patch to build")

    clashes = {(entry.record_type, entry.key) for entry in selections} & {
        (entry.record_type, entry.key) for entry in merges
    }
    if clashes:
        listed = ", ".join(f"{kind} {key!r}" for kind, key in sorted(clashes))
        raise PatchServiceError(
            f"{listed} is both taken whole and merged. Carrying it twice would "
            "leave the patch's own last-wins to decide which you get."
        )

    say(f"records: {len(selections)} whole, {len(merges)} merged")
    try:
        masters = _masters_for(selections, merges, records_by_plugin, load_order)
        say(f"declaring {len(masters)} master(s): {', '.join(masters)}")
        records = collect(selections, records_by_plugin, masters)
        for entry in merges:
            for line in describe(entry.choices, entry.base_plugin):
                say(f"  {entry.record_type} {entry.key}: {line}")
            records.append(
                merge_record(
                    entry.base_plugin,
                    entry.record_type,
                    entry.key,
                    entry.choices,
                    records_by_plugin,
                    masters,
                )
            )
    except PatchError as exc:
        raise PatchServiceError(str(exc)) from exc

    # Once over the whole patch, not once per source plugin: the notes are a
    # property of what is being carried, so looping the sources repeated every
    # note as many times as there were plugins.
    for note in dialogue_position_risk(records, records_by_plugin):
        say(f"  note: {note}")
    for key, anchor, plugin in position_anchors(records, records_by_plugin):
        say(
            f"  anchor: {key} sits next to {anchor[:12]}..., which {plugin} "
            "carries unchanged only to hold this line's place. The patch does "
            "not carry it, so that position stays whatever the load order "
            "makes it."
        )

    remapped = sum(1 for record in records if record.get("references"))
    if remapped:
        say(f"{remapped} record(s) had their references renumbered for the new master list")

    missing = [name for name in masters if name not in sizes]
    if missing:
        raise PatchServiceError(
            f"cannot measure {', '.join(missing)}. A master's size goes in the "
            "header, and a wrong one makes the plugin look corrupt."
        )

    try:
        document = build_plugin(
            records, [(name, sizes[name]) for name in masters], description=DESCRIPTION
        )
    except EmitError as exc:
        raise PatchServiceError(str(exc)) from exc

    result = PatchResult(records=len(records), masters=masters, remapped=remapped, lines=lines)
    if dry_run:
        say("dry run: nothing was written")
        return result

    _write(document, output, converter)
    say(f"wrote {output} ({output.stat().st_size} bytes, {len(document)} records)")
    result.output = output
    return result


def _masters_for(
    selections: Sequence[Selection],
    merges: Sequence[Merge],
    records_by_plugin: Mapping[str, Sequence[Mapping[str, Any]]],
    load_order: Sequence[str],
) -> list[str]:
    """Work out what a patch of these records and merges must declare.

    A merge reads from several plugins, and every one of them has to be in the
    master list -- otherwise the references taken from it cannot be renumbered
    and the record is refused.

    Args:
        selections: Whole records being carried.
        merges: Records being built from several plugins.
        records_by_plugin: The source plugins' decoded records.
        load_order: The full load order, which decides the result's order.

    Returns:
        The masters to declare, in load order.
    """
    stand_ins = list(selections)
    for entry in merges:
        stand_ins.extend(
            Selection(plugin=name, record_type=entry.record_type, key=entry.key)
            for name in sorted(entry.plugins)
        )
    return required_masters(stand_ins, records_by_plugin, load_order)


def _write(document: Sequence[Mapping[str, Any]], target: Path, converter: str) -> None:
    """Serialise the records and let tes3conv encode them.

    Args:
        document: The header and records.
        target: The plugin to write.
        converter: The tes3conv executable.

    Raises:
        PatchServiceError: If the conversion fails.
    """
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as scratch:
        as_json = Path(scratch) / "patch.json"
        as_json.write_text(json.dumps(list(document)), encoding="utf-8")
        try:
            result = subprocess.run(  # noqa: S603 -- fixed argv, no shell
                [converter, str(as_json), str(target), "--overwrite"],
                capture_output=True,
                text=True,
                timeout=600,
                check=False,
            )
        except (OSError, subprocess.SubprocessError) as exc:
            raise PatchServiceError(f"tes3conv could not be run: {exc}") from exc
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()[:300]
        raise PatchServiceError(f"tes3conv refused the patch: {detail}")
    if not target.is_file():
        raise PatchServiceError("tes3conv reported success but wrote no file")
