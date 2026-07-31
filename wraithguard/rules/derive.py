"""Turning what the tool observed into rules worth proposing.

Two tiers, kept apart on purpose, because they deserve different trust:

**Facts.** A plugin's master list is stored in the plugin's own header. If
``B.esp`` masters ``A.esp`` then the game will not load ``B`` without ``A``, and
``A`` loads first -- both are certainties read out of the file, not inferences.
Rules derived from them state something already true.

**Candidates.** The conflict scanner knows which plugins write the same records,
and that is *evidence of a relationship* without saying what the relationship
is. Two mods editing the same cell might want ordering one way, the other way,
or might be genuinely incompatible; the file cannot say which, and neither can
this module. So a candidate carries its evidence, proposes the reading the
current load order implies, offers the alternatives, and waits for a person.

The distinction is the whole design. A tool that presented both tiers the same
way would be inviting someone to rubber-stamp guesses, and a wrong rule in a
load-order file is worse than no rule -- it is a wrong answer that looks
researched.

Every proposal starts with its evidence in the citation field. That is a
*starting point*, not a citation: the guidelines want a readme, a forum post or
a URL so somebody else can check the claim, and "the tool noticed this" is not
that. :func:`needs_citation` reports when the field has not been improved on, so
a caller can insist before writing.
"""

from __future__ import annotations

from collections import Counter
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Final, Literal

from wraithguard.rules.authoring import Plugin, Rule, all_of

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

#: The base game's own files. A rule saying a mod requires Morrowind.esm is
#: noise: nothing loads without it, and the rule-base does not carry such rules.
BASE_GAME_MASTERS: Final[frozenset[str]] = frozenset(
    {"morrowind.esm", "tribunal.esm", "bloodmoon.esm"}
)

#: Words that suggest a plugin exists to patch other plugins. Only ever used to
#: *raise* a proposal to a person, never to write a rule on its own -- a name is
#: not evidence, which is why anything resting on this is a candidate.
_PATCH_WORDS: Final[frozenset[str]] = frozenset(
    {"patch", "compat", "compatibility", "fix", "fixes", "glue"}
)

Confidence = Literal["fact", "candidate"]

#: One record the conflict scanner reported. Only the two keys this module
#: reads are named; the scanner attaches more.
ConflictRecord = Mapping[str, Any]


@dataclass(frozen=True, slots=True)
class Proposal:
    """A rule the tool suggests, with everything needed to judge it.

    Attributes:
        rule: The rule as it would be written.
        confidence: ``"fact"`` when read straight out of the plugin files,
            ``"candidate"`` when inferred from observed behaviour.
        reason: Why this rule is being proposed, in one line.
        evidence: What was actually observed, in the terms it was observed in --
            master lists, record counts, record types.
        alternatives: Other readings of the same evidence, for a candidate. The
            usual case is the same ``[Order]`` the other way round.
    """

    rule: Rule
    confidence: Confidence
    reason: str
    evidence: str
    alternatives: tuple[Rule, ...] = field(default_factory=tuple)

    @property
    def is_fact(self) -> bool:
        """Whether this came from the files rather than from inference."""
        return self.confidence == "fact"


def needs_citation(proposal: Proposal) -> bool:
    """Whether a proposal still lacks a real citation.

    The evidence is prefilled into the rule's ``ref`` so the field is never
    blank, but evidence is not a citation: the guidelines ask for something a
    reader can go and check. This reports when the field is still only what the
    tool put there.

    Args:
        proposal: The proposal.

    Returns:
        ``True`` when the citation has not been replaced or added to.
    """
    return proposal.rule.ref.strip() == proposal.evidence.strip()


def _is_base_game(name: str) -> bool:
    """Whether a filename is one of the base game's masters.

    Args:
        name: The plugin filename.

    Returns:
        ``True`` for Morrowind, Tribunal or Bloodmoon.
    """
    return name.strip().lower() in BASE_GAME_MASTERS


def _active_only(names: Iterable[str], active_lower: frozenset[str]) -> list[str]:
    """Keep the names that are in the load order.

    Args:
        names: Candidate filenames.
        active_lower: Lower-cased active plugin filenames.

    Returns:
        The subset that is active, in the given order.
    """
    return [name for name in names if name.lower() in active_lower]


def order_from_masters(
    masters_by_plugin: Mapping[str, Sequence[str]],
    active: Sequence[str],
) -> list[Proposal]:
    """Derive ``[Order]`` rules from master dependencies.

    A plugin's masters are recorded in its header and the game loads them first.
    Ordering a plugin after its master is therefore a statement of fact, and the
    rule is worth writing down for anyone whose load order does not already have
    it that way.

    Args:
        masters_by_plugin: Plugin filename to the masters it declares.
        active: The active load order.

    Returns:
        One proposal per plugin that masters at least one other active mod. Base
        game masters are excluded: nothing loads without them and the rule-base
        does not carry such rules.
    """
    active_lower = frozenset(name.lower() for name in active)
    proposals: list[Proposal] = []
    for plugin, masters in masters_by_plugin.items():
        relevant = [
            master
            for master in _active_only(masters, active_lower)
            if not _is_base_game(master) and master.lower() != plugin.lower()
        ]
        if not relevant:
            continue
        evidence = f"{plugin} declares {', '.join(relevant)} as master(s) in its header"
        proposals.extend(
            Proposal(
                rule=Rule("Order", plugins=[master, plugin], ref=evidence),
                confidence="fact",
                reason=f"{plugin} masters {master}, so it must load after it",
                evidence=evidence,
            )
            for master in relevant
        )
    return proposals


def requires_from_masters(
    masters_by_plugin: Mapping[str, Sequence[str]],
    active: Sequence[str],
) -> list[Proposal]:
    """Derive ``[Requires]`` rules from master dependencies.

    Also a fact: the plugin names the master in its own header, and will not
    load without it.

    Args:
        masters_by_plugin: Plugin filename to the masters it declares.
        active: The active load order, used only to skip masters that are not
            mods (the base game).

    Returns:
        One proposal per plugin with non-base-game masters. Several masters
        become one rule with an ``[ALL ...]`` consequent, which is what the
        guidelines' own examples do.
    """
    del active
    proposals: list[Proposal] = []
    for plugin, masters in masters_by_plugin.items():
        relevant = [
            master
            for master in masters
            if not _is_base_game(master) and master.lower() != plugin.lower()
        ]
        if not relevant:
            continue
        consequent = (
            Plugin(relevant[0]) if len(relevant) == 1 else all_of(*[Plugin(m) for m in relevant])
        )
        evidence = f"{plugin} declares {', '.join(relevant)} as master(s) in its header"
        proposals.append(
            Proposal(
                rule=Rule(
                    "Requires",
                    expressions=[Plugin(plugin), consequent],
                    message=f"{plugin} requires {', '.join(relevant)}",
                    ref=evidence,
                ),
                confidence="fact",
                reason=f"{plugin} cannot load without {', '.join(relevant)}",
                evidence=evidence,
            )
        )
    return proposals


def patch_candidates(
    masters_by_plugin: Mapping[str, Sequence[str]],
    active: Sequence[str],
) -> list[Proposal]:
    """Propose ``[Patch]`` rules for plugins that look like compatibility patches.

    Half fact, half guess, and it is labelled a candidate for the guessed half.
    That a plugin masters two other active mods is read from its header. That
    this makes it a *patch* rather than an expansion of both is inferred from
    its filename, and a filename is not evidence.

    Args:
        masters_by_plugin: Plugin filename to the masters it declares.
        active: The active load order.

    Returns:
        One proposal per plausible patch.
    """
    active_lower = frozenset(name.lower() for name in active)
    proposals: list[Proposal] = []
    for plugin, masters in masters_by_plugin.items():
        stem = plugin.rsplit(".", 1)[0].lower()
        words = set(stem.replace("-", " ").replace("_", " ").split())
        if not (words & _PATCH_WORDS):
            continue
        originals = [
            master
            for master in _active_only(masters, active_lower)
            if not _is_base_game(master) and master.lower() != plugin.lower()
        ]
        if len(originals) < 2:
            continue
        evidence = f"{plugin} masters {', '.join(originals)}, and its name suggests a patch"
        proposals.append(
            Proposal(
                rule=Rule(
                    "Patch",
                    expressions=[Plugin(plugin), all_of(*[Plugin(o) for o in originals])],
                    message=f"{plugin} patches {' and '.join(originals)} together",
                    ref=evidence,
                ),
                confidence="candidate",
                reason="masters two or more active mods and is named like a patch",
                evidence=evidence,
            )
        )
    return proposals


def _conflict_pairs(
    conflicts: Iterable[ConflictRecord],
) -> dict[tuple[str, str], Counter[str]]:
    """Count conflicting records per pair of plugins, by record type.

    Args:
        conflicts: Conflict records as the scanner produces them, each with
            ``plugins`` and ``type``.

    Returns:
        ``(earlier, later)`` in load order to a count of record types. The pair
        is keyed in the order the scanner listed the plugins, which is load
        order, so the key itself carries the direction.
    """
    pairs: dict[tuple[str, str], Counter[str]] = {}
    for conflict in conflicts:
        raw = conflict.get("plugins")
        # A bare string is iterable, and iterating it yields characters -- so a
        # record whose "plugins" is "A.esp" rather than ["A.esp"] produced ten
        # proposals about single letters, presented to the user as suggestions.
        # Confident nonsense is the one output this module must never produce.
        if isinstance(raw, str) or not isinstance(raw, (list, tuple)):
            continue
        names = [str(name) for name in raw]
        rectype = str(conflict.get("type") or "?")
        for i, earlier in enumerate(names):
            for later in names[i + 1 :]:
                pairs.setdefault((earlier, later), Counter())[rectype] += 1
    return pairs


def order_candidates_from_conflicts(
    conflicts: Iterable[ConflictRecord],
    minimum_records: int = 1,
) -> list[Proposal]:
    """Propose ``[Order]`` rules for plugins that edit the same records.

    Editing the same record is evidence that two mods have a relationship. It is
    not evidence of *which* relationship: the later plugin wins, so the current
    order already encodes somebody's answer, but whether that answer is right is
    exactly the question. The proposal therefore states the current order, and
    carries the reverse as an alternative, so accepting is a decision rather
    than a default.

    Args:
        conflicts: Conflict records from the scanner.
        minimum_records: Ignore pairs with fewer conflicting records than this.
            One shared record is often coincidence; a caller showing a long list
            will want to raise it.

    Returns:
        Proposals, busiest pair first, so the most consequential decisions are
        the ones a person sees.
    """
    proposals: list[Proposal] = []
    for (earlier, later), types in _conflict_pairs(conflicts).items():
        total = sum(types.values())
        if total < minimum_records:
            continue
        breakdown = ", ".join(f"{count} {name}" for name, count in types.most_common())
        evidence = f"{total} record(s) edited by both {earlier} and {later}: {breakdown}"
        proposals.append(
            Proposal(
                rule=Rule(
                    "Order",
                    plugins=[earlier, later],
                    comment=f"{later} currently wins these records",
                    ref=evidence,
                ),
                confidence="candidate",
                reason=(
                    f"{earlier} and {later} edit {total} of the same record(s); "
                    f"whichever loads later wins them"
                ),
                evidence=evidence,
                alternatives=(
                    Rule(
                        "Order",
                        plugins=[later, earlier],
                        comment=f"{earlier} would win these records instead",
                        ref=evidence,
                    ),
                    Rule(
                        "Conflict",
                        expressions=[Plugin(earlier), Plugin(later)],
                        message=f"{earlier} and {later} edit the same records",
                        ref=evidence,
                    ),
                ),
            )
        )
    proposals.sort(key=lambda p: -int(p.evidence.split(" ", 1)[0]))
    return proposals


def propose_all(
    masters_by_plugin: Mapping[str, Sequence[str]],
    active: Sequence[str],
    conflicts: Iterable[ConflictRecord] = (),
    minimum_records: int = 1,
) -> list[Proposal]:
    """Gather every proposal, facts first.

    Args:
        masters_by_plugin: Plugin filename to its declared masters.
        active: The active load order.
        conflicts: Conflict records from the scanner, if a scan has run.
        minimum_records: Threshold for conflict-derived proposals.

    Returns:
        Facts first, then candidates. Ordering matters here: the reader should
        meet the things that are certainly true before the things that need
        their judgement.
    """
    facts = [
        *order_from_masters(masters_by_plugin, active),
        *requires_from_masters(masters_by_plugin, active),
    ]
    candidates = [
        *patch_candidates(masters_by_plugin, active),
        *order_candidates_from_conflicts(conflicts, minimum_records),
    ]
    return [*facts, *candidates]
