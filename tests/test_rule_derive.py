"""Tests for deriving rules from what the tool observed.

The point of the two-tier design is that facts and guesses are not treated the
same, so most of these tests are about that line holding:

* a **fact** comes out of a plugin header and is true whatever anyone thinks;
* a **candidate** comes from observed behaviour, carries its evidence, and
  offers the other readings rather than picking one silently.

The last class is the one that would catch a real regression: every proposal,
of either tier, has to be a rule the authoring validator accepts *and* the
engine acts on. A proposal that cannot be written is worse than none, because
it wastes the judgement someone just spent on it.
"""

from __future__ import annotations

import pytest

from wraithguard.rules import check_predicates, load_rule_blocks
from wraithguard.rules.authoring import errors, render_rule, validate
from wraithguard.rules.derive import (
    BASE_GAME_MASTERS,
    Proposal,
    needs_citation,
    order_candidates_from_conflicts,
    order_from_masters,
    patch_candidates,
    propose_all,
    requires_from_masters,
)

#: A small load order: a mod, another mod, and a patch that masters both.
MASTERS = {
    "BigMod.esp": ["Morrowind.esm"],
    "OtherMod.esp": ["Morrowind.esm"],
    "BigMod Patch.esp": ["Morrowind.esm", "BigMod.esp", "OtherMod.esp"],
}
ACTIVE = ["Morrowind.esm", "BigMod.esp", "OtherMod.esp", "BigMod Patch.esp"]

CONFLICTS = [
    {"type": "Cell", "id": "balmora", "plugins": ["BigMod.esp", "OtherMod.esp"]},
    {"type": "Cell", "id": "vivec", "plugins": ["BigMod.esp", "OtherMod.esp"]},
    {"type": "Npc", "id": "guard", "plugins": ["BigMod.esp", "OtherMod.esp"]},
]


class TestFactsFromMasters:
    """A master list is in the file; nothing here is inferred."""

    def test_a_master_produces_an_order_rule(self) -> None:
        """The game loads masters first, so this states something already true."""
        proposals = order_from_masters(MASTERS, ACTIVE)
        pairs = [tuple(p.rule.plugins) for p in proposals]

        assert ("BigMod.esp", "BigMod Patch.esp") in pairs
        assert ("OtherMod.esp", "BigMod Patch.esp") in pairs

    def test_master_derived_rules_are_facts(self) -> None:
        """They must not be presented like the guesses."""
        assert all(p.is_fact for p in order_from_masters(MASTERS, ACTIVE))
        assert all(p.is_fact for p in requires_from_masters(MASTERS, ACTIVE))

    @pytest.mark.parametrize("base", sorted(BASE_GAME_MASTERS))
    def test_the_base_game_is_never_proposed(self, base: str) -> None:
        """Nothing loads without Morrowind.esm; such a rule is noise.

        Args:
            base: The base game master under test.
        """
        proposals = order_from_masters({"Mod.esp": [base]}, ["Mod.esp", base])

        assert proposals == []

    def test_several_masters_become_one_requires_with_all(self) -> None:
        """Which is the shape the guidelines' own examples use."""
        proposals = requires_from_masters(MASTERS, ACTIVE)
        patch = next(
            p for p in proposals if p.rule.expressions[0].plugins() == ["BigMod Patch.esp"]
        )

        assert "[ALL BigMod.esp OtherMod.esp]" in render_rule(patch.rule)

    def test_a_single_master_needs_no_group(self) -> None:
        """``[ALL x]`` around one thing says nothing extra."""
        proposals = requires_from_masters({"A.esp": ["B.esp"]}, ["A.esp", "B.esp"])

        assert "[ALL" not in render_rule(proposals[0].rule)

    def test_a_plugin_mastering_itself_is_ignored(self) -> None:
        """Malformed, and a rule about it would be a cycle."""
        assert order_from_masters({"A.esp": ["A.esp"]}, ["A.esp"]) == []

    def test_an_inactive_master_produces_no_order_rule(self) -> None:
        """Ordering against something not in the load order orders nothing."""
        assert order_from_masters({"A.esp": ["Absent.esp"]}, ["A.esp"]) == []


class TestCandidatesFromConflicts:
    """Evidence of a relationship, not evidence of which relationship."""

    def test_a_conflicting_pair_is_proposed_as_a_candidate(self) -> None:
        """It needs judgement, so it must not claim to be a fact."""
        proposals = order_candidates_from_conflicts(CONFLICTS)

        assert len(proposals) == 1
        assert not proposals[0].is_fact

    def test_the_evidence_counts_records_by_type(self) -> None:
        """ "Trust me" is not evidence; the numbers are."""
        proposal = order_candidates_from_conflicts(CONFLICTS)[0]

        assert "3 record(s)" in proposal.evidence
        assert "2 Cell" in proposal.evidence
        assert "1 Npc" in proposal.evidence

    def test_the_proposal_follows_the_current_load_order(self) -> None:
        """Someone already answered this; the proposal shows their answer."""
        proposal = order_candidates_from_conflicts(CONFLICTS)[0]

        assert proposal.rule.plugins == ["BigMod.esp", "OtherMod.esp"]

    def test_the_reverse_order_is_offered_as_an_alternative(self) -> None:
        """So accepting is a decision rather than a default."""
        proposal = order_candidates_from_conflicts(CONFLICTS)[0]
        reversed_rules = [
            alt for alt in proposal.alternatives if alt.plugins == ["OtherMod.esp", "BigMod.esp"]
        ]

        assert reversed_rules, "the other direction must be one click away"

    def test_a_conflict_rule_is_offered_too(self) -> None:
        """Sometimes two mods should not be used together at all."""
        proposal = order_candidates_from_conflicts(CONFLICTS)[0]

        assert any(alt.kind == "Conflict" for alt in proposal.alternatives)

    def test_the_threshold_filters_incidental_overlap(self) -> None:
        """One shared record is often coincidence."""
        one = [{"type": "Cell", "id": "x", "plugins": ["A.esp", "B.esp"]}]

        assert order_candidates_from_conflicts(one, minimum_records=1)
        assert order_candidates_from_conflicts(one, minimum_records=2) == []

    def test_the_busiest_pair_comes_first(self) -> None:
        """The most consequential decision should be the one seen first."""
        conflicts = [
            *CONFLICTS,
            {"type": "Cell", "id": "q", "plugins": ["C.esp", "D.esp"]},
        ]
        proposals = order_candidates_from_conflicts(conflicts)

        assert proposals[0].rule.plugins == ["BigMod.esp", "OtherMod.esp"]

    def test_a_three_way_conflict_becomes_pairs(self) -> None:
        """An [Order] rule relates two plugins; three need three statements."""
        three = [{"type": "Cell", "id": "x", "plugins": ["A.esp", "B.esp", "C.esp"]}]
        pairs = {tuple(p.rule.plugins) for p in order_candidates_from_conflicts(three)}

        assert pairs == {("A.esp", "B.esp"), ("A.esp", "C.esp"), ("B.esp", "C.esp")}

    def test_no_conflicts_proposes_nothing(self) -> None:
        """A clean load order needs no rules written about it."""
        assert order_candidates_from_conflicts([]) == []


class TestPatchCandidates:
    """Half read from the file, half guessed from the name."""

    def test_a_patch_named_plugin_mastering_two_mods_is_proposed(self) -> None:
        """The masters are fact; that it is a *patch* is the guess."""
        proposals = patch_candidates(MASTERS, ACTIVE)

        assert len(proposals) == 1
        assert proposals[0].rule.kind == "Patch"

    def test_it_is_a_candidate_not_a_fact(self) -> None:
        """A filename is not evidence, and the tier has to say so."""
        assert not patch_candidates(MASTERS, ACTIVE)[0].is_fact

    def test_the_evidence_admits_the_name_was_used(self) -> None:
        """The reader should know which half of this was guessed."""
        assert "name suggests a patch" in patch_candidates(MASTERS, ACTIVE)[0].evidence

    def test_a_patch_named_plugin_with_one_master_is_not_proposed(self) -> None:
        """A [Patch] glues things together; one original is a [Requires]."""
        masters = {"Some Patch.esp": ["Morrowind.esm", "BigMod.esp"]}

        assert patch_candidates(masters, ["BigMod.esp", "Some Patch.esp"]) == []

    def test_an_ordinary_mod_is_not_proposed_as_a_patch(self) -> None:
        """Mastering two mods is normal; being called a patch is the signal."""
        masters = {"Expansion.esp": ["Morrowind.esm", "BigMod.esp", "OtherMod.esp"]}

        assert patch_candidates(masters, ACTIVE) == []


class TestCitations:
    """The guidelines want a source someone can check."""

    def test_the_evidence_is_prefilled_as_a_starting_point(self) -> None:
        """An empty field invites an empty citation."""
        for proposal in propose_all(MASTERS, ACTIVE, CONFLICTS):
            assert proposal.rule.ref.strip()

    def test_an_unedited_citation_is_reported(self) -> None:
        """ "The tool noticed this" is not a source."""
        proposal = propose_all(MASTERS, ACTIVE, CONFLICTS)[0]

        assert needs_citation(proposal)

    def test_an_edited_citation_satisfies_the_check(self) -> None:
        """Once a person names the readme, the rule can be written."""
        proposal = propose_all(MASTERS, ACTIVE, CONFLICTS)[0]
        proposal.rule.ref = "BigMod readme.txt"

        assert not needs_citation(proposal)


class TestOrdering:
    """Facts before guesses, everywhere."""

    def test_facts_come_before_candidates(self) -> None:
        """Meet what is certainly true before what needs judgement."""
        proposals = propose_all(MASTERS, ACTIVE, CONFLICTS)
        tiers = [p.confidence for p in proposals]

        assert tiers == sorted(tiers, key=lambda t: t != "fact")

    def test_both_tiers_are_present_in_this_fixture(self) -> None:
        """Otherwise the ordering test above would pass vacuously."""
        tiers = {p.confidence for p in propose_all(MASTERS, ACTIVE, CONFLICTS)}

        assert tiers == {"fact", "candidate"}

    def test_conflicts_are_optional(self) -> None:
        """Proposals must work before any scan has been run."""
        assert propose_all(MASTERS, ACTIVE)


class TestEveryProposalIsWritableAndWorks:
    """The property that makes the feature worth anything.

    A proposal that fails validation, or that mlox reads and ignores, wastes
    exactly the thing the design asks a person to spend: their judgement.
    """

    ALL = propose_all(MASTERS, ACTIVE, CONFLICTS)

    def test_the_fixture_produces_proposals(self) -> None:
        """Guards the tests below from passing over an empty list."""
        assert len(self.ALL) >= 5

    def test_every_proposed_rule_validates(self) -> None:
        """Including the alternatives, which are one click from being written."""
        for proposal in self.ALL:
            for rule in (proposal.rule, *proposal.alternatives):
                problems = errors(validate(rule))
                assert not problems, f"{render_rule(rule)}\n{[p.describe() for p in problems]}"

    def test_every_ordering_proposal_produces_a_chain(self, tmp_path) -> None:
        """The sort has to see it, or the rule orders nothing.

        Args:
            tmp_path: Pytest temporary directory.
        """
        ordering = [p for p in self.ALL if p.rule.kind == "Order"]
        assert ordering, "the fixture should produce ordering rules"

        for proposal in ordering:
            path = tmp_path / "r.txt"
            path.write_text(render_rule(proposal.rule) + "\n", encoding="utf-8")
            blocks, _nearstart, _nearend = load_rule_blocks([path])

            assert blocks == [(proposal.rule.plugins, 0)], render_rule(proposal.rule)

    def test_every_warning_proposal_fires(self) -> None:
        """A derived [Requires] that never warns has told nobody anything."""
        warning_rules = [p for p in self.ALL if p.rule.kind in ("Requires", "Patch")]
        assert warning_rules, "the fixture should produce warning rules"

        for proposal in warning_rules:
            # The dependant present, its requirement absent: the state each of
            # these rules exists to complain about.
            dependant = proposal.rule.expressions[0].plugins()[0]
            warnings = check_predicates(render_rule(proposal.rule), [dependant])

            assert warnings, f"did not fire:\n{render_rule(proposal.rule)}"

    def test_no_proposal_carries_an_empty_citation(self) -> None:
        """Rendering would drop it, leaving an untraceable rule."""
        for proposal in self.ALL:
            assert "(Ref:" in render_rule(proposal.rule) or proposal.rule.kind == "Order"


class TestProposalShape:
    """The dataclass itself."""

    def test_is_fact_matches_the_confidence(self) -> None:
        """One is derived from the other; they must not disagree."""
        from wraithguard.rules.authoring import Rule

        assert Proposal(Rule("Order"), "fact", "r", "e").is_fact
        assert not Proposal(Rule("Order"), "candidate", "r", "e").is_fact

    def test_alternatives_default_to_none(self) -> None:
        """A fact has no other reading."""
        from wraithguard.rules.authoring import Rule

        assert Proposal(Rule("Order"), "fact", "r", "e").alternatives == ()


class TestMalformedScannerRecords:
    """Garbage in must not become confident nonsense out.

    Found during the 3.1 audit by feeding the module records the scanner would
    never produce. It matters more here than in most places: this module's whole
    design is about not presenting guesses as facts, and a proposal built from
    junk is the strongest possible version of that failure.
    """

    def test_a_string_of_plugins_is_not_iterated_by_character(self) -> None:
        """ "A.esp" instead of ["A.esp"] produced ten proposals about letters."""
        records = [{"type": "Cell", "id": "x", "plugins": "A.esp"}]

        assert order_candidates_from_conflicts(records) == []

    @pytest.mark.parametrize("value", [None, 42, {"a": 1}, object()])
    def test_a_non_sequence_is_skipped(self, value: object) -> None:
        """Better to propose nothing than to propose something invented.

        Args:
            value: The malformed ``plugins`` value.
        """
        records = [{"type": "Cell", "id": "x", "plugins": value}]

        assert order_candidates_from_conflicts(records) == []

    def test_a_tuple_is_accepted_like_a_list(self) -> None:
        """The guard must reject strings, not sequences in general."""
        records = [{"type": "Cell", "id": "x", "plugins": ("A.esp", "B.esp")}]

        assert len(order_candidates_from_conflicts(records)) == 1

    def test_a_missing_type_still_proposes(self) -> None:
        """The record type is only used for the evidence text."""
        records = [{"plugins": ["A.esp", "B.esp"]}]
        proposals = order_candidates_from_conflicts(records)

        assert len(proposals) == 1
        assert "?" in proposals[0].evidence

    def test_one_good_record_among_bad_ones_still_counts(self) -> None:
        """Skipping a malformed record must not abandon the whole scan."""
        records = [
            {"type": "Cell", "plugins": "junk"},
            {"type": "Cell", "plugins": ["A.esp", "B.esp"]},
        ]

        assert len(order_candidates_from_conflicts(records)) == 1
