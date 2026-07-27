"""Emit ``momw-customizations.toml``.

Generates the customisations file the MOMW Configurator consumes: the insert
blocks for the user's own plugins, their ``data=`` paths, and any removals --
while preserving verbatim everything in the source TOML this tool does not
own.

That preservation is deliberate. The file is hand-edited, and silently
dropping a block the tool does not understand would lose the user's work.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from mlox_subset.configurator.apply import configurator_remove_matches
from mlox_subset.configurator.cfglines import (
    cfg_line_value,
    extract_data_path_value,
    normalize_data_path,
    toml_value,
)
from mlox_subset.i18n import gettext as _

if TYPE_CHECKING:
    from collections.abc import Collection, Mapping, Sequence


def _subset_runs(
    final_content_order: Sequence[str],
    subset_lower: Collection[str],
    replace_dest_names: Collection[str],
) -> list[tuple[int, int]]:
    """Group the subset's plugins into runs of consecutive positions.

    Each run becomes one ``insertBlock`` on one anchor. A plugin handled by a
    ``replace`` block breaks a run, because it is emitted elsewhere and must not
    appear in an insert as well.

    Args:
        final_content_order: The whole sorted ``content=`` order.
        subset_lower: Lower-cased names of the user's own plugins.
        replace_dest_names: Plugins emitted via ``replace`` instead.

    Returns:
        Half-open ``(start, end)`` index pairs, in order.
    """
    runs: list[tuple[int, int]] = []
    start: int | None = None
    for index, name in enumerate(final_content_order):
        mine = name.lower() in subset_lower and name not in replace_dest_names
        if mine and start is None:
            start = index
        elif not mine and start is not None:
            runs.append((start, index))
            start = None
    if start is not None:
        runs.append((start, len(final_content_order)))
    return runs


def _anchor_is_unique(anchor: str, haystack: Sequence[str]) -> bool:
    """Whether an anchor matches exactly one cfg line.

    momw-configurator matches anchors with ``strings.Contains`` against whole
    lines and treats more than one match as fatal for the entire run, so an
    ambiguous anchor is not a warning to the user -- it is a failed rebuild.

    Args:
        anchor: The candidate ``after``/``before`` value.
        haystack: The cfg lines the Configurator will be matching against.

    Returns:
        ``True`` if exactly one line contains it.
    """
    return sum(1 for line in haystack if anchor in line) == 1


def _pick_anchor(
    final_content_order: Sequence[str],
    start: int,
    end: int,
    haystack: Sequence[str],
) -> tuple[str, str | None]:
    """Choose an unambiguous anchor for one run of inserts.

    Preference order, and the reasoning for it:

    1. ``after`` the plugin immediately before the run -- the natural choice,
       and the one that reads correctly in the file.
    2. ``before`` the plugin immediately after it. Places the run identically
       and gives a second chance at a unique line.
    3. Neither, when the run is the whole file.

    An ambiguous candidate is skipped rather than emitted: the run's position is
    the same either way, so there is no cost to preferring the one that works.
    Where *both* neighbours are ambiguous the natural anchor is emitted anyway
    and the existing ambiguity warning fires -- the alternative would be to
    silently drop the insert, which is worse than a rebuild that stops and says
    so.

    Args:
        final_content_order: The whole sorted ``content=`` order.
        start: First index of the run.
        end: One past the last index of the run.
        haystack: The cfg lines anchors are matched against.

    Returns:
        ``(mode, anchor)`` where mode is ``"after"`` or ``"before"``; the anchor
        is ``None`` when the run covers the entire order.
    """
    before_run = final_content_order[start - 1] if start > 0 else None
    after_run = final_content_order[end] if end < len(final_content_order) else None
    if before_run is not None and _anchor_is_unique(before_run, haystack):
        return "after", before_run
    if after_run is not None and _anchor_is_unique(after_run, haystack):
        return "before", after_run
    if before_run is not None:
        return "after", before_run
    if after_run is not None:
        return "before", after_run
    return "after", None


def generate_customizations_toml(
    original_data: Mapping[str, Any] | None,
    final_content_order: Sequence[str],
    subset_set: Collection[str],
    original_content_values: Mapping[str, str],
    data_result_tuples: Sequence[tuple[str, bool, str | None]] | None = None,
    raw_data_inserts: Sequence[Mapping[str, Any]] | None = None,
    replace_dest_names: Collection[str] | None = None,
    user_data_values: Collection[str] | None = None,
    list_name: str | None = None,
    remove_content: Sequence[str] | None = None,
    remove_data: Sequence[str] | None = None,
    custom_anchors: Mapping[str, Any] | None = None,
    new_groundcover: Sequence[str] | None = None,
) -> str:
    """Generate the ``momw-customizations.toml`` the Configurator consumes.

    Everything this tool does not own is preserved verbatim from
    ``original_data`` -- ``replace`` and ``append`` blocks, the ``remove*``
    keys, ``listName``. The file is hand-edited, so silently dropping a block
    we do not understand would lose the user's work.

    Args:
        original_data: Parsed source TOML. Its ``listName``, ``remove*``,
            ``replace`` and ``append`` blocks are carried through unchanged.
        final_content_order: The full mlox-sorted ``content=`` plugin list.
        subset_set: Which of those are the user's own, and therefore need a
            regenerated insert block.
        original_content_values: ``{plugin_name: original insert value}``, so
            whatever the user originally wrote is kept -- usually identical to
            the name, but not always.
        data_result_tuples: Output of
            :func:`~mlox_subset.configurator.datapaths.insert_data_paths`, used
            when data paths were sorted, to emit re-anchored inserts.
        raw_data_inserts: The original insert dicts, used when data paths were
            *not* sorted, to pass them through unchanged.
        replace_dest_names: Plugins that arrived as a ``replace`` *dest* rather
            than an ``insert``. They are emitted via the replace passthrough,
            so an insert block here would duplicate and conflict with it.
            Note that momw-configurator's ``replace`` has no ``after``/
            ``before`` of its own -- it inherits the position of ``source`` --
            so mlox moving one of these cannot be expressed as a replace at
            all, and is reported as a warning instead.
        user_data_values: Raw paths of every ``data=`` insert from this run,
            *before* duplicate-skipping. Required for correctness: a path the
            Configurator already baked into ``openmw.cfg`` on a previous run is
            correctly skipped as a live-cfg duplicate, but must still be
            re-emitted here. Without it the regenerated TOML would silently
            lose every data path already in the cfg -- which is all of them,
            since the cfg was built from this very file -- leaving the
            Configurator nothing to re-insert on the next rebuild.
        list_name: Overrides ``listName``. momw-configurator requires it;
            precedence is this argument, then the source TOML, then
            ``"generated"``.
        remove_content: ``removeContent`` entries to emit.
        remove_data: ``removeData`` entries to emit.
        custom_anchors: Per-plugin anchor overrides.
        new_groundcover: Plugins this run declares as grass that the cfg does not
            already have. Emitted as ``append`` entries (``groundcover=NAME``),
            which is how the Configurator writes a groundcover line -- there is
            no insert form for one. Their **data paths are not handled here**:
            those go through the ordinary data-insert path, because OpenMW has
            to be able to find the file and a data= entry is not grass-specific.

    Returns:
        The complete TOML document, newline-terminated.
    """
    replace_dest_names = replace_dest_names or set()
    subset_set_lower = {s.lower() for s in subset_set}
    original_data = original_data or {}
    # No existing [[Customizations]] block to attach inserts to (e.g. --subset-file
    # was used with no --customizations at all) -- synthesize one so there's
    # somewhere for the insert/replace/append output below to actually go,
    # instead of the whole loop silently iterating zero times.
    # listName is REQUIRED by momw-configurator (it says which curated list the
    # customizations apply to). Precedence: an explicit list_name passed in
    # (--list-name / GUI field) wins; else keep whatever the source TOML had;
    # else fall back to "generated" so the file is at least valid TOML. The
    # override also covers the --subset-file-only case, which otherwise always
    # emitted the useless placeholder "generated".
    default_name = list_name or "generated"
    blocks = original_data.get("Customizations") or [{"listName": default_name}]

    # extra removals from opted-out items that already exist in the cfg -- added
    # to the FIRST block only, so a multi-block file doesn't repeat them
    extra_removes = {
        "removeContent": list(remove_content or []),
        "removeData": list(remove_data or []),
    }

    # The lines momw-configurator will match anchors against. Built before the
    # emit loop, not after it, because choosing an anchor now depends on knowing
    # whether it is unique -- the check at the end of this function only warns.
    haystack_for_anchors = [f"content={n}" for n in final_content_order]
    if data_result_tuples:
        haystack_for_anchors += [line for line, _is_new, _val in data_result_tuples]

    out = []
    _anchors = []  # every after=/before=/source= value we emit, for the ambiguity check
    _removes = []  # every remove* value we emit -- removal matching is SILENT
    for bi, block in enumerate(blocks):
        out.append("[[Customizations]]")
        name = list_name or block.get("listName")
        if name:
            out.append(f"listName = {toml_value(name)}")
        for key in ("removeData", "removeContent", "removeFallback", "removeGroundcover"):
            vals = list(block.get(key) or [])
            if bi == 0:
                vals += extra_removes.get(key, [])
            # de-dupe case-insensitively, preserving order
            seen, merged = set(), []
            for x in vals:
                if x.lower() not in seen:
                    seen.add(x.lower())
                    merged.append(x)
            if merged:
                # one entry per line, matching the style of MOMW's own
                # documentation examples -- a 25-entry single line is unreadable
                out.append(f"{key} = [")
                out.extend(f"  {toml_value(x)}," for x in merged)
                out.append("]")
                _removes.extend(merged)
        out.append("")

        # 1) DATA INSERTS FIRST (Ensures paths are defined before plugins look for them)
        if data_result_tuples:
            # We emit a block for every line that's OURS -- a genuinely new
            # insert, OR an existing cfg line whose path is one of this run's
            # data paths (i.e. one momw-configurator already applied on a prior
            # run). The latter is why we can't just gate on is_new: after the
            # first rebuild every one of our paths is "already in the cfg", and
            # gating on is_new would drop them all from the regenerated TOML.
            user_norms = {normalize_data_path(v) for v in (user_data_values or [])}
            user_norms.discard("")

            def _anchor_val(entry: tuple[str, bool, str | None]) -> str:
                """The path an entry should be anchored against.

                Falls back to splitting the raw line when the value cannot be
                extracted, so a malformed line still yields *something* to
                anchor on rather than aborting the emit.
                """
                aline, _is_new, _value = entry
                return extract_data_path_value(aline) or aline.split("=", 1)[-1].strip().strip('"')

            classified = []
            for entry in data_result_tuples:
                line, is_new, value = entry
                path_val = value if value else extract_data_path_value(line)
                norm = normalize_data_path(path_val) if path_val else ""
                is_ours = bool(path_val) and (is_new or (norm and norm in user_norms))
                classified.append((entry, path_val, is_ours))

            # Anchoring each new insert "after" the insert immediately before
            # it (as this used to do) is fragile: if two consecutive new
            # paths happen to share a text prefix -- e.g.
            # '...\OpenMW_SetBonus' and '...\OpenMW_SetBonusRebalance' --
            # the first one's own path is a literal substring of the
            # second's cfg line, so momw-configurator's whole-line substring
            # anchor match finds 2 hits for it and aborts the whole apply.
            # So instead, every entry in a contiguous run of new inserts
            # anchors to the SAME existing (frozen, never another new
            # insert) neighboring line:
            #   - a frozen line follows the run -> anchor the whole run
            #     "before" it, emitted in forward order. momw-configurator
            #     inserts each one right before that same target in turn,
            #     which lands them in the correct final order (verified
            #     against simulate_configurator_apply).
            #   - otherwise (run is at the very end of the data= list) ->
            #     anchor the whole run "after" the preceding frozen line
            #     instead, emitted in REVERSE order (same mechanism,
            #     mirrored).
            # Either way, every anchor this emits is a path that existed in
            # openmw.cfg before this run touched it -- never another new
            # insert's own path -- so this whole collision class can't recur.
            i, n = 0, len(classified)
            while i < n:
                entry, path_val, is_ours = classified[i]
                if not is_ours:
                    i += 1
                    continue
                j = i
                while j < n and classified[j][2]:
                    j += 1
                run = classified[i:j]  # contiguous "ours" entries, in final order
                next_frozen = classified[j] if j < n else None
                prev_frozen = classified[i - 1] if i > 0 else None
                if next_frozen is not None:
                    anchor = _anchor_val(next_frozen[0])
                    ordered = run
                    mode = "before"
                elif prev_frozen is not None:
                    anchor = _anchor_val(prev_frozen[0])
                    ordered = list(reversed(run))
                    mode = "after"
                else:
                    anchor = None
                for _entry, val, _ours in ordered if anchor is not None else run:
                    # `run` holds only entries whose is_ours is True, and
                    # is_ours requires a truthy path_val -- so `val` cannot be
                    # None here. Checked rather than assumed: emitting
                    # toml_value(None) would write the literal string 'None'
                    # into the cfg as a data path, which fails silently.
                    if not val:
                        continue
                    out.append("[[Customizations.insert]]")
                    out.append(f"insert = {toml_value(val)}")
                    if anchor is not None:
                        out.append(f"{mode} = {toml_value(anchor)}")
                        _anchors.append(anchor)
                    else:
                        out.append(
                            "# WARNING: no existing data= line anywhere in the cfg to anchor to"
                        )
                    out.append("")
                i = j
        elif raw_data_inserts:
            # --sort-data-paths not given -- pass these through exactly as originally written
            for d in raw_data_inserts:
                out.append("[[Customizations.insert]]")
                out.append(f"insert = {toml_value(d['value'])}")
                if d.get("after"):
                    out.append(f"after = {toml_value(d['after'])}")
                    _anchors.append(d["after"])
                elif d.get("before"):
                    out.append(f"before = {toml_value(d['before'])}")
                    _anchors.append(d["before"])
                out.append("")

        # 2) CONTENT INSERTS SECOND
        # The subset's plugins go in as insertBlock runs rather than one insert
        # per plugin chained on its predecessor.
        #
        # Why: `after`/`before` are matched with strings.Contains against WHOLE
        # cfg lines, and >1 match makes momw-configurator abandon the cfg it was
        # building. Chaining anchored every plugin on the *previously inserted
        # plugin name*, so each one was another chance to hit that -- and it is
        # a real shape, not a hypothetical: inserting 'Wares.esp' into a list
        # that already ships 'Better Wares.esp' matches both lines.
        #
        # A run of consecutive subset plugins is one block on one anchor, and
        # that anchor is a line the sort already placed, so it can be checked
        # (see _pick_anchor). Equivalence with the chained form is proven in
        # tests/test_toml_equivalence.py by applying both through
        # simulate_configurator_apply and diffing the resulting cfg.
        for start, end in _subset_runs(final_content_order, subset_set_lower, replace_dest_names):
            names = final_content_order[start:end]
            values = [original_content_values.get(n, n) for n in names]

            # Keep the per-plugin "why is it here" annotations. They explain the
            # sort's reasoning, which is per plugin even when the insert is not.
            for name in names:
                info = (custom_anchors or {}).get(name.lower())
                if not info:
                    continue
                how, anch = info
                if how == "after":
                    out.append(f"# {name}: must load after {toml_value(anch)}")
                elif how == "before":
                    out.append(f"# {name}: must load before {toml_value(anch)}")
                elif how in ("nearstart", "nearend"):
                    label = "NearStart" if how == "nearstart" else "NearEnd"
                    out.append(f"# {name}: mlox [{label}] hint")
                else:
                    out.append(f"# {name}: no ordering constraint -- positional only")

            mode, anchor = _pick_anchor(final_content_order, start, end, haystack_for_anchors)
            out.append("[[Customizations.insert]]")
            if len(values) == 1:
                # A single plugin needs no block; `insert` is the plainer form
                # and applies identically.
                out.append(f"insert = {toml_value(values[0])}")
            else:
                body = "\n".join(values)
                out.append(f'insertBlock = """{body}"""')
            if anchor is None:
                out.append("# WARNING: this is the only content= plugin -- no anchor to write")
            else:
                out.append(f"{mode} = {toml_value(anchor)}")
                _anchors.append(anchor)
            out.append("")

        for rep in block.get("replace", []):
            out.append("[[Customizations.replace]]")
            if "source" in rep:
                out.append(f"source = {toml_value(rep['source'])}")
                _anchors.append(rep["source"])
            if "dest" in rep:
                out.append(f"dest = {toml_value(rep['dest'])}")
            out.append("")

        # Declared grass, as append entries. Only in the first block: repeating
        # them per block would write the line once per customization.
        if bi == 0:
            for name in new_groundcover or []:
                out.append("[[Customizations.append]]")
                out.append(f"append = {toml_value(f'groundcover={name}')}")
                out.append("")

        for ap in block.get("append", []):
            out.append("[[Customizations.append]]")
            if "append" in ap:
                out.append(f"append = {toml_value(ap['append'])}")
            if "appendBlock" in ap:
                out.append(f"appendBlock = {toml_value(ap['appendBlock'])}")
            out.append("")

    # Ambiguity checks (warn-only, output unchanged). Verified against
    # momw-configurator's cfg/custom.go:
    #  * after=/before=/source= values are matched with strings.Contains
    #    against WHOLE cfg lines, and >1 match is a hard error (doInsert even
    #    discards the cfg it was building) -- so a filename nested inside
    #    another ('Incantation.omwscripts' in 'content=Incantation.omwscripts.esp')
    #    breaks the configurator run.
    #  * remove* values match the same way but with NO multi-match error --
    #    doRemove silently deletes EVERY matching line, so a nested filename
    #    would silently remove a mod the user never opted out. (Path-like
    #    values instead match the line's value exactly or by /-suffix.)
    haystack = haystack_for_anchors

    _line_value = cfg_line_value
    _remove_matches = configurator_remove_matches

    for a in dict.fromkeys(_anchors):  # dedupe, keep order
        hits = [line for line in haystack if a in line]
        if len(hits) > 1:
            print(
                _(
                    "WARNING: anchor '%(anchor)s' in the emitted TOML matches "
                    "%(count)d openmw.cfg lines -- momw-configurator errors on "
                    "ambiguous matches. Colliding lines: %(lines)s%(more)s"
                )
                % {
                    "anchor": a,
                    "count": len(hits),
                    "lines": "; ".join(hits[:4]),
                    "more": " ..." if len(hits) > 4 else "",
                }
            )
    for r in dict.fromkeys(_removes):
        hits = [line for line in haystack if _remove_matches(r, line)]
        if len(hits) > 1:
            print(
                _(
                    "WARNING: remove entry '%(entry)s' matches %(count)d openmw.cfg "
                    "lines -- momw-configurator removes ALL of them, silently. "
                    "Colliding lines: %(lines)s%(more)s"
                )
                % {
                    "entry": r,
                    "count": len(hits),
                    "lines": "; ".join(hits[:4]),
                    "more": " ..." if len(hits) > 4 else "",
                }
            )

    return "\n".join(out).rstrip() + "\n"
