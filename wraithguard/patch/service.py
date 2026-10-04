"""One call that turns chosen records into a patch plugin.

The GUI and any future command-line caller need the same four steps in the same
order, and neither should own them: read the source plugins, work out what the
patch must declare, carry the records across with their references remapped,
and write one new file.

**Everything it does is additive.** No source plugin is opened for writing. The
output is one new file that loads last; deleting it restores the previous
behaviour exactly. The only destructive act available is overwriting a previous
patch of the same name, and that is the caller's decision.

**A patch can be built over several sessions.** Pass an earlier build of the
same output file as ``carried`` and its records ride along into the new one,
so today's queue only has to hold today's decisions -- last week's are already
in the file being rewritten. See :func:`wraithguard.patch.records.carry_forward`
for why that needs different handling than an ordinary source plugin.
"""

from __future__ import annotations

import copy
import json
import logging
import subprocess
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final

from wraithguard.land.emit import EmitError, build_plugin
from wraithguard.patch.dialogue import shifts as dialogue_shifts
from wraithguard.patch.merge import Merge, describe, merge_record
from wraithguard.patch.records import (
    PatchError,
    Selection,
    carry_forward,
    collect,
    dialogue_position_risk,
    keys_of,
    master_names,
    position_anchors,
    required_masters,
)
from wraithguard.patch.refedit import cell_patch_record, place_new_refs
from wraithguard.proc import no_window_kwargs

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence

    from wraithguard.patch.records import NewRecord
    from wraithguard.patch.refedit import NewRef, RefEdit

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
        records: How many records it carries in total.
        carried: How many of those came from an earlier build of this same
            patch rather than this session's own selections and merges. Zero
            for a fresh patch or a replace.
        masters: The files it declares, in order.
        remapped: Records whose references had to be renumbered.
        lines: The progress report, for a log panel.
    """

    output: Path | None = None
    records: int = 0
    carried: int = 0
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
    carried: Sequence[Mapping[str, Any]] = (),
    dry_run: bool = False,
    report: Callable[[str], None] | None = None,
    ref_edits: Sequence[RefEdit] = (),
    new_refs: Sequence[NewRef] = (),
    new_records: Sequence[NewRecord] = (),
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
        converter: Path to ``tes3conv``, which does the binary encoding, or ``""``
            for the Rust backend.
        output: Where to write.
        carried: An earlier build of *this same* patch, decoded, header
            included -- pass this to append rather than replace. Every record
            in it is re-emitted into the new build, except any this call's own
            ``selections``/``merges`` decide too, which take precedence: a
            record re-decided this session should carry the new answer, not
            both. Pass the previous version's own records, not some other
            plugin's -- see :func:`wraithguard.patch.records.carry_forward`
            for why that distinction matters.
        dry_run: Report without writing.
        report: Called with each progress line, if given.
        ref_edits: Changes to placed objects (:mod:`wraithguard.patch.refedit`). Each
            cell gets a CELL record carrying only its changed references - added to
            the cell's record when the patch carries that cell anyway (whole, merged,
            or from the earlier build). ``records_by_plugin`` must hold every plugin
            with a CELL record for those cells (``RefEdit.plugins``). A reference the
            patch itself created (``origin`` is ``output``'s name) stays ``(0, n)``.
        new_refs: References the patch adds (:class:`.refedit.NewRef`), each ``(0, n)``
            numbered on from the highest the patch carries, the plugin defining what it
            places a master (``base_plugin``).
        new_records: Records the patch makes itself (:class:`.records.NewRecord`),
            written as they are, the plugin each was copied from a master.

    Returns:
        What was produced.

    Raises:
        PatchServiceError: If the patch cannot be built or written.
    """
    lines: list[str] = []

    def say(text: str) -> None:
        """Record one progress line and pass it on.

        Kept for the caller either way: the report callback is optional, but
        the collected lines go back in the result, so a failure can be read
        after the fact by anything that was not watching live.

        Args:
            text: The line to record.
        """
        lines.append(text)
        if report is not None:
            report(text)

    if not (selections or merges or carried or ref_edits or new_refs or new_records):
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

    say(
        f"records: {len(selections)} whole, {len(merges)} merged"
        + (f", up to {len(carried)} carried forward from the existing patch" if carried else "")
    )
    try:
        own = output.name
        sources = [
            Selection(plugin=m.source, record_type=m.record_type, key=m.key)
            for m in new_records
            if m.source
        ]
        masters = _masters_for(
            [*selections, *sources],
            merges,
            records_by_plugin,
            load_order,
            carried,
            ref_edits,
            new_refs,
            own,
        )
        say(f"declaring {len(masters)} master(s): {', '.join(masters)}")

        # This session's own decisions always win: a record carried forward
        # that was also re-decided here would otherwise sit next to its own
        # replacement, leaving the patch's last-wins to pick between them.
        decided = frozenset(
            {(entry.record_type, entry.key) for entry in selections}
            | {(entry.record_type, entry.key) for entry in merges}
        )
        made = frozenset((m.record_type, m.key) for m in new_records)
        records = carry_forward(carried, masters, skip=decided | made) if carried else []
        carried_count = len(records)
        if carried:
            say(f"{carried_count} record(s) carried forward from the existing patch")

        records.extend(collect(selections, records_by_plugin, masters))
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
        for m in new_records:
            say(f"  {m.record_type} {m.key}: made by the patch")
            records.append(copy.deepcopy(dict(m.record)))
        if ref_edits:
            _apply_ref_edits(records, ref_edits, records_by_plugin, load_order, masters, say, own)
        if new_refs:
            given = place_new_refs(records, new_refs, records_by_plugin, load_order)
            for new in new_refs:
                say(f"  Cell {new.cell}: new {new.fields.get('id')} as (0, {given[new.uid]})")
    except PatchError as exc:
        raise PatchServiceError(str(exc)) from exc

    # Once over the whole patch, not once per source plugin: the notes are a
    # property of what is being carried, so looping the sources repeated every
    # note as many times as there were plugins.
    for note in dialogue_position_risk(records, records_by_plugin):
        say(f"  note: {note}")
    # Where the hedging stops. The engine's placement rule is simple enough to
    # replay, so instead of saying a position "depends on" other files, resolve
    # the topic with and without the patch and report the actual numbers.
    for topic, key, was, now in dialogue_shifts(records_by_plugin, load_order, records):
        say(
            f"  moves: {key} in topic {topic!r} goes from position {was} to "
            f"{now}. A topic is read top down and the first response whose "
            "filters match is the one spoken."
        )
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

    result = PatchResult(
        records=len(records),
        carried=carried_count,
        masters=masters,
        remapped=remapped,
        lines=lines,
    )
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
    carried: Sequence[Mapping[str, Any]] = (),
    ref_edits: Sequence[RefEdit] = (),
    new_refs: Sequence[NewRef] = (),
    own: str = "",
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
        carried: An earlier build of this same patch, if appending to one.
            Its own masters join the required set even though nothing here
            takes a record from it *as* a plugin -- the carried records still
            reference those masters by position.
        ref_edits: Changes to placed objects: each reference is named by the file
            that created it, so that file is a master - unless it is the patch itself.
        new_refs: References the patch adds: what each places is defined by a master.
        own: The patch's own file name, never its own master.

    Returns:
        The masters to declare, in load order.
    """
    stand_ins = list(selections)
    for entry in merges:
        stand_ins.extend(
            Selection(plugin=name, record_type=entry.record_type, key=entry.key)
            for name in sorted(entry.plugins)
        )
    stand_ins.extend(Selection(plugin=e.origin, record_type="Cell", key=e.cell) for e in ref_edits)
    stand_ins.extend(
        Selection(plugin=n.base_plugin, record_type="Cell", key=n.cell)
        for n in new_refs
        if n.base_plugin
    )
    if own:
        stand_ins = [s for s in stand_ins if s.plugin.lower() != own.lower()]
    found = required_masters(stand_ins, records_by_plugin, load_order, extra=master_names(carried))
    return [name for name in found if not own or name.lower() != own.lower()]


def _apply_ref_edits(
    records: list[dict[str, Any]],
    ref_edits: Sequence[RefEdit],
    records_by_plugin: Mapping[str, Sequence[Mapping[str, Any]]],
    load_order: Sequence[str],
    masters: Sequence[str],
    say: Callable[[str], None],
    own: str = "",
) -> None:
    """Put each cell's changed references into the patch.

    A cell the patch already carries (whole, merged, or from the earlier build) gets
    them in its own record, a reference with the same key replaced; any other cell
    gets a CELL record of its own with only them (:func:`.refedit.cell_patch_record`).

    Args:
        records: The patch's records so far (changed in place).
        ref_edits: The changes.
        records_by_plugin: The plugins' decoded records.
        load_order: The load order.
        masters: The patch's masters.
        say: The progress reporter.
        own: The patch's own file name (its references are ``(0, n)``).
    """
    by_cell: dict[str, list[RefEdit]] = {}
    for e in ref_edits:
        by_cell.setdefault(e.cell.lower(), []).append(e)
    for edits in by_cell.values():
        built = cell_patch_record(edits, records_by_plugin, load_order, masters, own)
        cell = edits[0].cell
        say(f"  Cell {cell}: {len(edits)} changed reference(s)")
        have = next(
            (
                r
                for r in records
                if r.get("type") == "Cell" and cell.lower() in {k.lower() for k in keys_of(r)}
            ),
            None,
        )
        if have is None:
            records.append(built)
            continue
        key = lambda r: (r.get("mast_index"), r.get("refr_index"))  # noqa: E731 - one use
        changed = {key(r): r for r in built["references"]}
        kept = [r for r in have.get("references") or [] if key(r) not in changed]
        have["references"] = kept + list(changed.values())


def _write(document: Sequence[Mapping[str, Any]], target: Path, converter: str) -> None:
    """Encode the records into the plugin at ``target``.

    With no ``converter`` (the native session's case), the Rust backend writes it:
    the records are built into record objects and :func:`~wraithguard.esp.write_plugin`
    encodes them, as the Merged Lands writer does. With a tes3conv executable, the
    records go to it as JSON.

    Args:
        document: The header and records.
        target: The plugin to write.
        converter: The tes3conv executable, or ``""`` to encode natively.

    Raises:
        PatchServiceError: If the encoding fails.
    """
    target.parent.mkdir(parents=True, exist_ok=True)
    if not converter:
        from wraithguard.esp import EspError, plugin_from_json, write_plugin

        try:
            target.write_bytes(write_plugin(plugin_from_json([dict(r) for r in document])))
        except (OSError, EspError, ValueError, KeyError, TypeError) as exc:
            raise PatchServiceError(f"could not encode the patch natively: {exc}") from exc
        return
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
                # No console flash when a --noconsole build writes the patch.
                **no_window_kwargs(),
            )
        except (OSError, subprocess.SubprocessError) as exc:
            raise PatchServiceError(f"tes3conv could not be run: {exc}") from exc
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()[:300]
        raise PatchServiceError(f"tes3conv refused the patch: {detail}")
    if not target.is_file():
        raise PatchServiceError("tes3conv reported success but wrote no file")
