"""The Editor's verifier (wraithguard.patch.verify) over what the patch carries."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wraithguard.patch import verify as vf
from wraithguard.patch.editor import EditorSession
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path

PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {
            "type": "Npc",
            "id": "fargoth",
            "name": "Fargoth",
            "race": "Wood Elf",
            "class": "",
            "script": "",
            "inventory": [[1, "gold_001"]],
            "spells": [],
        },
        {"type": "Race", "id": "Wood Elf", "name": "Wood Elf", "spells": []},
        {"type": "MiscItem", "id": "gold_001", "name": "Gold"},
        {"type": "LeveledItem", "id": "l_gold", "items": [["gold_001", 1]]},
        {"type": "Cell", "name": "Vault", "data": {"grid": [0, 0], "flags": "IS_INTERIOR"}},
        {
            "type": "PathGrid",
            "flags": "",
            "cell": "Vault",
            "data": {"grid": [0, 0], "granularity": 1024, "point_count": 0},
            "points": [],
            "connections": [],
        },
        *(
            {"type": "GlobalVariable", "id": g, "value": {"type": "Short", "data": 0}}
            for g in ("Day", "DaysPassed", "GameHour", "Month", "PCRace")
        ),
    ],
}
_TAGS = {
    "NPC_": "Npc",
    "RACE": "Race",
    "MISC": "MiscItem",
    "LEVI": "LeveledItem",
    "CELL": "Cell",
    "PGRD": "PathGrid",
    "GLOB": "GlobalVariable",
}


def _session(tmp_path: Path) -> EditorSession:
    """A session over :data:`PLUGINS`."""
    return EditorSession(
        [(n, tmp_path / n) for n in PLUGINS],
        PatchQueue(),
        read=lambda p, tag: [r for r in PLUGINS[p.name] if r["type"] == _TAGS.get(tag)],
    )


def _checks(found: list[dict[str, str]]) -> list[tuple[str, str, str]]:
    """``(check, id, path)`` of each finding."""
    return sorted((f["check"], f["id"], f["path"]) for f in found)


def test_nothing_queued_nothing_found(tmp_path):
    assert vf.verify(_session(tmp_path)) == {"findings": [], "checked": 0}


def test_fields_naming_missing_records(tmp_path):
    s = _session(tmp_path)
    npc = s.find("NPC_", "fargoth")
    assert npc is not None
    s.set_field(npc, "script", "NoSuchScript")
    s.set_field(npc, "inventory", [[1, "gold_001"], [2, "no_such_item"]])
    got = vf.verify(s)
    assert got["checked"] == 1
    assert _checks(got["findings"]) == [
        ("Missing ID", "fargoth", "inventory.1.1"),
        ("Missing ID", "fargoth", "script"),
    ]
    # The race and the gold exist: not findings. A record the patch makes counts too.
    s.insert("MISC", "no_such_item")
    assert _checks(vf.verify(s)["findings"]) == [("Missing ID", "fargoth", "script")]


def test_leveled_list_entries(tmp_path):
    s = _session(tmp_path)
    lev = s.find("LEVI", "l_gold")
    assert lev is not None
    s.set_field(lev, "items", [["gold_001", 1], ["", 2]])
    assert _checks(vf.verify(s)["findings"]) == [("Leveled list", "l_gold", "items.1")]
    s.set_field(lev, "items", [])
    found = vf.verify(s)["findings"]
    assert [(f["level"], f["path"]) for f in found] == [("warning", "items")]


def test_path_grid_points(tmp_path):
    s = _session(tmp_path)
    s.set_pathgrid("int:Vault", [[0, 0, 0], [0, 0, 0], [100, 0, 0]], [[0, 2], [2, 0]])
    msgs = sorted(f["message"] for f in vf.verify(s)["findings"])
    assert msgs == ["point 1 is on point 0 (0, 0, 0)", "point 1 is unlinked"]


def test_path_grid_bad_links():
    found = vf.path_grid("PGRD", "x", [[0, 0, 0], [1, 0, 0]], [[0, 1], [1, 0], [1, 5]])
    assert [f["message"] for f in found] == ["a link 1-5 to a missing point"]


def test_dialogue_chain():
    topic = {"id": "rumors", "responses": [{"id": "1"}, {"id": "2", "orphan": True}]}
    assert _checks(vf.dialogue_chain(topic, {"1", "2"})) == [("Dialogue", "2", "prev_id")]
    assert vf.dialogue_chain(topic, {"1"}) == []


def test_journal_stages():
    topic = {
        "id": "MS_Q",
        "type": "Journal",
        "responses": [
            {"id": "a", "quest": "Name", "text": "A Quest", "disposition": 0},
            {"id": "b", "quest": "Name", "text": "Again", "disposition": 0},
            {"id": "c", "text": "Started", "disposition": 10},
            {"id": "d", "text": "", "disposition": 10},
        ],
    }
    assert sorted(f["message"] for f in vf.journal(topic)) == [
        "index 10 twice (c, d)",
        "more than one quest name",
        "stage 10 has no text",
    ]


def test_record_checks():
    def msgs(rtype: str, rec: dict[str, Any], files: Any = None) -> list[str]:
        return sorted(f["message"] for f in vf.record_checks("X", "x", rtype, rec, files))

    assert msgs(
        "Class",
        {
            "name": "C",
            "data": {
                "attribute1": "Luck",
                "attribute2": "Luck",
                "major1": "Axe",
                "minor2": "Axe",
                "flags": "PLAYABLE",
            },
        },
    ) == [
        "Axe is listed twice among the skills",
        "a playable class has no description",
        "the same attribute is listed twice",
    ]
    assert msgs("Region", {"name": "R", "weather_chances": {"clear": 50, "rain": 40}}) == [
        "the weather chances add up to 90, not 100"
    ]
    assert msgs(
        "Race", {"name": "R", "description": "d", "data": {"height": [1, 0], "weight": [-1, 1]}}
    ) == [
        "the female height is not above 0",
        "the male weight is below 0",
    ]
    assert msgs("Spell", {"name": ""}) == ["no magic effects", "the name is missing"]
    assert msgs("GameSetting", {"value": {"type": "Integer", "data": 1}}) == []
    assert (
        vf.record_checks("GMST", "fX", "GameSetting", {"value": {"type": "Integer", "data": 1}})[0][
            "message"
        ]
        == "fX is a Integer setting; its name says Float"
    )
    assert msgs(
        "Enchanting",
        {"effects": [{}], "data": {"cost": 10, "max_charge": 5, "enchant_type": "CastWhenUsed"}},
    ) == ["each use costs 10, more than its charge of 5"]
    assert (
        msgs(
            "Enchanting",
            {
                "effects": [{}],
                "data": {"cost": 10, "max_charge": 0, "enchant_type": "ConstantEffect"},
            },
        )
        == []
    )
    have = {"sound/fx/a.wav", "textures/b.dds"}
    assert msgs("Sound", {"sound_path": "Fx\\A.wav"}, have.__contains__) == []
    assert msgs("Sound", {"sound_path": "fx\\gone.wav"}, have.__contains__) == [
        "fx\\gone.wav is in no data folder or archive"
    ]
    assert (
        msgs("Birthsign", {"name": "B", "description": "d", "texture": "b.tga"}, have.__contains__)
        == []
    )


def test_enchant_capacity():
    rec = {"data": {"enchantment": 30}}
    assert vf.enchant_capacity("ARMO", "a", rec, 3, 0.1) == []
    found = vf.enchant_capacity("ARMO", "a", rec, 4, 0.1)
    assert found[0]["message"].startswith("its enchantment costs 4; the item carries 3")
    assert vf.enchant_capacity("ARMO", "a", rec, 4, None) == []


def test_under_ground():
    flat = [[100.0] * 65 for _ in range(65)]
    found = vf.under_ground("PGRD", "(1, 2)", [[64, 64, 120], [4000, 4000, 50]], flat)
    assert [f["message"] for f in found] == ["point 1 is 50 units under the ground"]


def test_filters_travel_and_ai_name_records():
    info = {
        "filters": [{"filter_type": "NotClass", "id": "Thief"}, {"filter_type": "Local", "id": "x"}]
    }
    assert [(p, t) for p, _, t in vf.references("DialogueInfo", info)] == [
        ("filters.0.id", ("CLAS",))
    ]
    npc = {
        "ai_packages": [
            {"type": "Follow", "target": "player2", "cell": "Vault"},
            {"type": "Wander"},
        ],
        "travel_destinations": [{"cell": ""}, {"cell": "Balmora, Inn"}],
    }
    paths = sorted(p for p, _, _ in vf.references("Npc", npc))
    assert paths == ["ai_packages.0.cell", "ai_packages.0.target", "travel_destinations.1.cell"]


def test_required_globals(tmp_path):
    def nothing(_tags: tuple[str, ...], _name: str) -> bool:
        return False

    assert [f["id"] for f in vf.required_ids(nothing)] == list(vf.REQUIRED_GLOBALS)


def test_placed_references_naming_nothing(tmp_path):
    s = _session(tmp_path)
    gold = s.find("MISC", "gold_001")
    assert gold is not None
    new = s.place("Vault", gold, [0, 0, 0])
    assert vf.verify(s)["findings"] == []
    # Search & Replace can give it another object; one nothing defines is a finding.
    from dataclasses import replace

    s.queue.add_new_ref(replace(new, fields={"id": "no_such_thing"}))
    assert [f["check"] for f in vf.verify(s)["findings"]] == ["Reference"]


def test_references_reach_into_lists():
    rec = {"biped_objects": [{"male_bodypart": "b_m", "female_bodypart": ""}], "script": "s"}
    assert sorted(p for p, _, _ in vf.references("Armor", rec)) == [
        "biped_objects.0.male_bodypart",
        "script",
    ]
