"""_tes3conv_record_key: turning one tes3conv JSON record into a (type, id)
conflict key.

The fallback chain matters more than any single branch: id, then name, then
grid coordinates, then a cell name -- and a cell-scoped record (path grids)
has its own trap the docstring calls out directly: every interior stores
grid (0, 0) by convention, so keying purely by coordinates collapses every
interior's pathgrid across every plugin into one bogus conflict. Getting the
interior/exterior distinction right (via interior_cells when available, via
the "no flag lookup available" grid-zero heuristic otherwise) is the actual
job of this function.
"""

from __future__ import annotations

import wraithguard_toolkit as core


class TestNoUsableKey:
    def test_a_non_dict_record_returns_none(self) -> None:
        assert core._tes3conv_record_key("not a dict") is None  # type: ignore[arg-type]

    def test_a_record_with_no_type_returns_none(self) -> None:
        assert core._tes3conv_record_key({"id": "torch_01"}) is None

    def test_a_header_record_returns_none(self) -> None:
        assert core._tes3conv_record_key({"type": "Header", "masters": []}) is None

    def test_a_header_type_is_matched_case_insensitively(self) -> None:
        assert core._tes3conv_record_key({"type": "HEADER"}) is None

    def test_a_tes3_record_returns_none(self) -> None:
        assert core._tes3conv_record_key({"type": "TES3"}) is None

    def test_a_record_with_nothing_to_key_on_at_all_returns_none(self) -> None:
        assert core._tes3conv_record_key({"type": "GameSetting"}) is None


class TestIdAndName:
    def test_a_record_with_an_id_is_keyed_on_it(self) -> None:
        assert core._tes3conv_record_key({"type": "Static", "id": "torch_01"}) == (
            "Static",
            "torch_01",
        )

    def test_a_record_with_only_a_name_is_keyed_on_that(self) -> None:
        # Some record types (Script) carry "name" rather than "id".
        assert core._tes3conv_record_key({"type": "Script", "name": "MyScript"}) == (
            "Script",
            "MyScript",
        )

    def test_an_id_present_but_empty_falls_through_to_the_next_source(self) -> None:
        result = core._tes3conv_record_key({"type": "Cell", "id": "", "grid": [3, -2]})
        assert result == ("Cell", "(3, -2)")


class TestGridFallback:
    def test_a_top_level_grid_is_used_when_there_is_no_id_or_cell(self) -> None:
        assert core._tes3conv_record_key({"type": "Cell", "grid": [3, -2]}) == (
            "Cell",
            "(3, -2)",
        )

    def test_a_grid_nested_under_data_is_also_found(self) -> None:
        assert core._tes3conv_record_key({"type": "Cell", "data": {"grid": [1, 1]}}) == (
            "Cell",
            "(1, 1)",
        )

    def test_a_grid_too_short_to_use_yields_no_key(self) -> None:
        assert core._tes3conv_record_key({"type": "Cell", "grid": [3]}) is None


class TestCellScopedRecordsWithInteriorCellsProvided:
    def test_a_cell_known_to_be_interior_is_keyed_by_name_alone(self) -> None:
        result = core._tes3conv_record_key(
            {"type": "PathGrid", "cell": "Balmora, Guild of Mages", "grid": [0, 0]},
            interior_cells={"balmora, guild of mages"},
        )
        assert result == ("PathGrid", "Balmora, Guild of Mages")

    def test_a_cell_known_to_be_exterior_is_keyed_by_name_and_coordinates(self) -> None:
        result = core._tes3conv_record_key(
            {"type": "PathGrid", "cell": "Named Region", "grid": [3, -2]},
            interior_cells=set(),
        )
        assert result == ("PathGrid", "Named Region (3, -2)")

    def test_the_interior_check_is_case_insensitive(self) -> None:
        result = core._tes3conv_record_key(
            {"type": "PathGrid", "cell": "Balmora, Guild Of Mages", "grid": [0, 0]},
            interior_cells={"balmora, guild of mages"},
        )
        assert result == ("PathGrid", "Balmora, Guild Of Mages")

    def test_a_cell_name_nested_under_data_is_also_found(self) -> None:
        result = core._tes3conv_record_key(
            {"type": "PathGrid", "data": {"cell": "Balmora, Guild"}, "grid": [0, 0]},
            interior_cells={"balmora, guild"},
        )
        assert result == ("PathGrid", "Balmora, Guild")


class TestCellScopedRecordsWithoutInteriorCells:
    def test_grid_zero_zero_falls_back_to_the_old_interior_heuristic(self) -> None:
        # interior_cells=None: no flag lookup available, so (0, 0) is
        # assumed interior -- the historical, slightly ambiguous behaviour.
        result = core._tes3conv_record_key(
            {"type": "PathGrid", "cell": "Some Interior", "grid": [0, 0]}
        )
        assert result == ("PathGrid", "Some Interior")

    def test_a_nonzero_grid_falls_back_to_the_old_exterior_heuristic(self) -> None:
        result = core._tes3conv_record_key(
            {"type": "PathGrid", "cell": "Named Region", "grid": [3, -2]}
        )
        assert result == ("PathGrid", "Named Region (3, -2)")
