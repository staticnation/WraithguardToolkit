"""Reading many records without re-parsing their plugins once each.

``Tes3ConvSession.record_map`` deliberately caches nothing: holding every
plugin's decoded records was multi-gigabyte on a real load order. That is the
right call for reading one record on demand, and it makes reading a *batch*
quadratic in the worst way -- judging 2,000 records that Morrowind.esm defines
re-parses its 183 MB JSON 2,000 times, and one cold parse is 14 seconds.

The sidecar cache already saves re-running tes3conv. Nothing was saving the
parse. So the batch reader inverts the loops: each plugin read once, only the
wanted records kept, the parse dropped before the next plugin is opened.

Measured on the real cache: 159 records across 3 plugins took 109s the old way
(477 parses) and 0.68s the new way (3 parses), with identical output.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

import wraithguard_toolkit as core  # noqa: E402


class Session:
    """A session that counts how often each plugin is read."""

    def __init__(self, plugins: dict[str, dict[tuple[str, str], Any]]) -> None:
        """Hold the records each plugin path would decode to."""
        self.plugins = plugins
        self.parses = 0

    def record_map(self, path: str) -> dict[tuple[str, str], Any]:
        """Mimic the real method: build fresh every call, cache nothing."""
        self.parses += 1
        return dict(self.plugins.get(str(path), {}))

    def record_subset(self, path: str, wanted: Any) -> dict[tuple[str, str], Any]:
        """Mimic the streaming reader: one parse, only the wanted records.

        Same one-read-per-plugin cost as ``record_map`` -- the streaming is a
        memory property, not a read-count one -- so the parse tallies the tests
        assert are unchanged.
        """
        self.parses += 1
        records = self.plugins.get(str(path), {})
        return {key: records[key] for key in wanted if key in records}


PATHS = {"Base.esm": "/base", "Mod.esp": "/mod"}

PLUGINS: dict[str, dict[tuple[str, str], Any]] = {
    "/base": {
        ("Armor", "cuirass"): {"type": "Armor", "id": "cuirass", "weight": 30},
        ("Armor", "greaves"): {"type": "Armor", "id": "greaves", "weight": 10},
    },
    "/mod": {
        ("Armor", "cuirass"): {"type": "Armor", "id": "cuirass", "weight": 18},
        ("Armor", "greaves"): {"type": "Armor", "id": "greaves", "weight": 10},
    },
}

CONFLICTS: list[dict[str, Any]] = [
    {"type": "Armor", "id": "cuirass", "plugins": ["Base.esm", "Mod.esp"]},
    {"type": "Armor", "id": "greaves", "plugins": ["Base.esm", "Mod.esp"]},
]


class TestEachPluginIsReadOnce:
    """The whole point of the function."""

    def test_two_records_cost_two_reads_not_four(self) -> None:
        """One per plugin, however many records are wanted from it."""
        session = Session(PLUGINS)
        core.batch_record_fields(session, CONFLICTS, PATHS)
        assert session.parses == 2

    def test_the_count_does_not_grow_with_the_records(self) -> None:
        """The property that makes a whole-load-order scan possible at all."""
        session = Session(PLUGINS)
        core.batch_record_fields(session, CONFLICTS * 50, PATHS)
        assert session.parses == 2

    def test_a_plugin_with_no_path_is_read_zero_times(self) -> None:
        """A scan can outlive the paths it was taken from."""
        session = Session(PLUGINS)
        core.batch_record_fields(
            session,
            [{"type": "Armor", "id": "cuirass", "plugins": ["Gone.esp"]}],
            PATHS,
        )
        assert session.parses == 0


class TestItAgreesWithTheOneAtATimeReader:
    """A faster answer is only worth having if it is the same answer."""

    def test_the_fields_match(self) -> None:
        """Same keys, same order."""
        one = core.diff_record_fields(Session(PLUGINS), CONFLICTS[0], PATHS)
        many = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS)
        assert many[("Armor", "cuirass")][0] == one[0]

    def test_the_values_match(self) -> None:
        """Per plugin, flattened the same way."""
        one = core.diff_record_fields(Session(PLUGINS), CONFLICTS[0], PATHS)
        many = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS)
        assert many[("Armor", "cuirass")][1] == one[1]

    def test_every_asked_record_comes_back(self) -> None:
        """A silently dropped record would read as an unjudged one."""
        many = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS)
        assert set(many) == {("Armor", "cuirass"), ("Armor", "greaves")}


class TestWhatItDoesWithGaps:
    """Real scans are ragged."""

    def test_a_record_a_plugin_lacks_reads_as_empty(self) -> None:
        """Not as missing from the result: the caller judges absence itself."""
        plugins = {"/base": PLUGINS["/base"], "/mod": {}}
        many = core.batch_record_fields(Session(plugins), CONFLICTS, PATHS)
        assert many[("Armor", "cuirass")][1]["Mod.esp"] == {}

    def test_no_session_reads_nothing(self) -> None:
        """The field-diff engine is optional; its absence is not an error."""
        assert core.batch_record_fields(None, CONFLICTS, PATHS) == {}

    def test_no_conflicts_reads_nothing(self) -> None:
        """An empty filter should not touch the disk at all."""
        session = Session(PLUGINS)
        assert core.batch_record_fields(session, [], PATHS) == {}
        assert session.parses == 0


class TestHashingInsteadOfHolding:
    """Judging compares for equality, so it never needs the values themselves.

    Holding them is what makes a whole load order run out of memory: a
    landscape's height and normal fields are tens of kilobytes of base64 each,
    and a real scan has 50,000 conflicting records across three plugins apiece.
    """

    def test_equal_values_stay_equal(self) -> None:
        """Or every unchanged field would read as a conflict."""
        many = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, digest=True)
        per = many[("Armor", "greaves")][1]
        assert per["Base.esm"]["weight"] == per["Mod.esp"]["weight"]

    def test_different_values_stay_different(self) -> None:
        """Or every real conflict would vanish, which is the worse failure."""
        many = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, digest=True)
        per = many[("Armor", "cuirass")][1]
        assert per["Base.esm"]["weight"] != per["Mod.esp"]["weight"]

    def test_the_fields_are_still_all_there(self) -> None:
        """Hashing the values must not lose the keys."""
        plain = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS)
        hashed = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, digest=True)
        assert hashed[("Armor", "cuirass")][0] == plain[("Armor", "cuirass")][0]

    def test_a_hash_is_small(self) -> None:
        """The whole point; a regression here is silent until it is an OOM."""
        many = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, digest=True)
        assert len(many[("Armor", "cuirass")][1]["Base.esm"]["weight"]) == 16

    def test_the_values_survive_when_not_digesting(self) -> None:
        """The display path needs them, so the flag must really be a flag."""
        many = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS)
        assert many[("Armor", "cuirass")][1]["Base.esm"]["weight"] == 30

    def test_judging_agrees_either_way(self) -> None:
        """The verdict must not depend on how the values were stored."""
        from wraithguard.patch.status import ConflictAll
        from wraithguard.patch.summary import field_statuses, record_status

        plugins = ["Base.esm", "Mod.esp"]
        verdicts = []
        for flag in (False, True):
            read = core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, digest=flag)
            keys, per = read[("Armor", "cuirass")]
            verdicts.append(record_status(field_statuses(keys, per, plugins)))
        assert verdicts[0] is verdicts[1] is ConflictAll.OVERRIDE_BENIGN


class TestTheLockIsHeldPerPluginNotPerCall:
    """The crash this argument exists to prevent.

    The tes3conv session is one process on one pipe, so concurrent readers must
    take turns. Wrapping the whole batch in the lock does that -- and holds it
    for the entire run, which on a real load order is minutes. A Plugin summary
    was doing exactly that when a click in the Plugin view asked for a record;
    the UI thread waited, stopped answering the window manager, and the
    application was reported as not responding and killed.

    So the lock is taken and released around each plugin read. Every other
    reader waits at most one plugin, never a whole scan.
    """

    class Counting:
        """A lock that records how many times it was entered and left."""

        def __init__(self) -> None:
            self.entered = 0
            self.left = 0
            self.held = 0
            self.most = 0

        def __enter__(self) -> Any:
            self.entered += 1
            self.held += 1
            self.most = max(self.most, self.held)
            return self

        def __exit__(self, *_exc: object) -> None:
            self.held -= 1
            self.left += 1

    def test_it_is_taken_once_per_plugin(self) -> None:
        """Two plugins, two acquisitions -- not one spanning both."""
        lock = self.Counting()
        core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, lock=lock)
        assert lock.entered == 2

    def test_it_is_released_every_time(self) -> None:
        """A leaked hold is a permanent freeze rather than a temporary one."""
        lock = self.Counting()
        core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, lock=lock)
        assert lock.left == lock.entered
        assert lock.held == 0

    def test_it_is_never_held_across_two_reads(self) -> None:
        """The property that bounds every other reader's wait to one plugin."""
        lock = self.Counting()
        core.batch_record_fields(Session(PLUGINS), CONFLICTS * 20, PATHS, lock=lock)
        assert lock.most == 1

    def test_no_lock_is_a_valid_choice(self) -> None:
        """Single-threaded callers should not have to invent one."""
        assert core.batch_record_fields(Session(PLUGINS), CONFLICTS, PATHS, lock=None)


class TestProgress:
    """A long read with no sign of movement looks like a hang."""

    def test_progress_counts_up_to_the_number_of_plugins(self) -> None:
        """Reporting records would show a bar that jumps, not one that moves."""
        seen: list[tuple[int, int]] = []
        core.batch_record_fields(
            Session(PLUGINS), CONFLICTS, PATHS, lambda done, total: seen.append((done, total))
        )
        assert seen == [(1, 2), (2, 2)]
