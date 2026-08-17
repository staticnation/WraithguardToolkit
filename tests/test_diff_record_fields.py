"""diff_record_fields: field-level comparison of one conflicting record
across every plugin that touches it.

Uses a minimal stub in place of a real Tes3ConvSession -- the function's
only contact with it is one call to ``record_subset(path, {key})`` per
plugin, so a fake with that single method is enough to test the real logic
(ordering, flattening, and what counts as "differs") without needing
tes3conv installed at all.
"""

from __future__ import annotations

from typing import Any

import wraithguard_toolkit as core


class _FakeSession:
    """Returns a canned record for whichever (path, key) the test configured.

    Args:
        records: path -> {key: record_dict}. A path/key combination not
            present here behaves like the record wasn't in that plugin.
    """

    def __init__(self, records: dict[str, dict[tuple[str, str], dict[str, Any]]]) -> None:
        self._records = records

    def record_subset(
        self, path: str, keys: set[tuple[str, str]]
    ) -> dict[tuple[str, str], dict[str, Any]]:
        found = self._records.get(path, {})
        return {k: v for k, v in found.items() if k in keys}


class TestNoSession:
    def test_none_session_returns_empty_everything(self) -> None:
        result = core.diff_record_fields(
            None, {"type": "Static", "id": "torch_01", "plugins": ["A.esp"]}, {"A.esp": "/a"}
        )
        assert result == ([], {}, set())


class TestAgreementAndDifference:
    def test_identical_values_across_plugins_are_not_differing(self) -> None:
        key = ("Static", "torch_01")
        session = _FakeSession(
            {
                "/a": {key: {"mesh": "torch.nif"}},
                "/b": {key: {"mesh": "torch.nif"}},
            }
        )
        conflict = {"type": "Static", "id": "torch_01", "plugins": ["A.esp", "B.esp"]}
        paths = {"A.esp": "/a", "B.esp": "/b"}

        ordered, per, differing = core.diff_record_fields(session, conflict, paths)

        assert ordered == ["mesh"]
        assert per == {"A.esp": {"mesh": "torch.nif"}, "B.esp": {"mesh": "torch.nif"}}
        assert differing == set()

    def test_a_different_value_is_reported_as_differing(self) -> None:
        key = ("Static", "torch_01")
        session = _FakeSession(
            {
                "/a": {key: {"mesh": "torch_a.nif"}},
                "/b": {key: {"mesh": "torch_b.nif"}},
            }
        )
        conflict = {"type": "Static", "id": "torch_01", "plugins": ["A.esp", "B.esp"]}
        paths = {"A.esp": "/a", "B.esp": "/b"}

        _ordered, _per, differing = core.diff_record_fields(session, conflict, paths)

        assert differing == {"mesh"}

    def test_a_field_missing_from_one_plugin_counts_as_differing(self) -> None:
        # Present in A, absent in B -- even though B has no conflicting
        # value, the field's presence itself differs and matters.
        key = ("Static", "torch_01")
        session = _FakeSession(
            {
                "/a": {key: {"mesh": "torch.nif", "extra_field": "x"}},
                "/b": {key: {"mesh": "torch.nif"}},
            }
        )
        conflict = {"type": "Static", "id": "torch_01", "plugins": ["A.esp", "B.esp"]}
        paths = {"A.esp": "/a", "B.esp": "/b"}

        _ordered, _per, differing = core.diff_record_fields(session, conflict, paths)

        assert "extra_field" in differing
        assert "mesh" not in differing

    def test_nested_dict_fields_are_flattened_with_dotted_keys(self) -> None:
        key = ("Cell", "Some Interior")
        session = _FakeSession({"/a": {key: {"data": {"grid": [0, 0]}}}})
        conflict = {"type": "Cell", "id": "Some Interior", "plugins": ["A.esp"]}
        paths = {"A.esp": "/a"}

        ordered, per, _differing = core.diff_record_fields(session, conflict, paths)

        assert "data.grid" in ordered
        assert per["A.esp"]["data.grid"] == [0, 0]


class TestMissingOrUnreadableSources:
    def test_a_plugin_with_no_known_path_gets_an_empty_field_set(self) -> None:
        key = ("Static", "torch_01")
        session = _FakeSession({"/a": {key: {"mesh": "torch.nif"}}})
        conflict = {"type": "Static", "id": "torch_01", "plugins": ["A.esp", "Ghost.esp"]}
        paths = {"A.esp": "/a"}  # Ghost.esp has no path at all

        _ordered, per, _differing = core.diff_record_fields(session, conflict, paths)

        assert per["Ghost.esp"] == {}

    def test_a_record_not_found_by_the_session_gets_an_empty_field_set(self) -> None:
        # The path is known, but this particular record isn't in that plugin
        # (e.g. record_subset genuinely found nothing for the key).
        session = _FakeSession({"/a": {}})
        conflict = {"type": "Static", "id": "torch_01", "plugins": ["A.esp"]}
        paths = {"A.esp": "/a"}

        _ordered, per, _differing = core.diff_record_fields(session, conflict, paths)

        assert per["A.esp"] == {}


class TestFieldOrder:
    def test_first_seen_order_follows_the_plugin_list_order(self) -> None:
        session = _FakeSession(
            {
                "/a": {("Static", "x"): {"b_field": 1}},
                "/b": {("Static", "x"): {"a_field": 2, "b_field": 1}},
            }
        )
        conflict = {"type": "Static", "id": "x", "plugins": ["A.esp", "B.esp"]}
        paths = {"A.esp": "/a", "B.esp": "/b"}

        ordered, _per, _differing = core.diff_record_fields(session, conflict, paths)

        # b_field appears first because A.esp is listed first and has it;
        # a_field only appears once B.esp is reached.
        assert ordered == ["b_field", "a_field"]
