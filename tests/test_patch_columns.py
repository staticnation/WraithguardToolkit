"""wraithguard.patch.columns: the Object Window's per-type columns."""

from __future__ import annotations

from wraithguard.patch.columns import table


def test_lockpick_columns_from_the_winner():
    base = {
        "pick_a": {
            "type": "Lockpick",
            "id": "pick_a",
            "name": "Pick",
            "data": {"weight": 0.25, "value": 10, "uses": 25, "quality": 1.0},
            "flags": "",
        }
    }
    later = {
        "pick_a": {
            "type": "Lockpick",
            "id": "pick_a",
            "name": "Better Pick",
            "data": {"weight": 0.25, "value": 20, "uses": 30, "quality": 1.25},
            "flags": "PERSISTENT",
        }
    }
    t = table("LOCK", [base, later])
    labels = [c["label"] for c in t["columns"]]
    assert labels[:6] == ["Name", "Script", "Weight", "Value", "Uses", "Quality"]
    row = dict(zip(labels, t["rows"]["pick_a"], strict=True))
    assert (
        row["Name"] == "Better Pick"
        and row["Uses"] == 30
        and row["Persists"] == "yes"
        and row["Blocked"] == "no"
    )


def test_lists_flags_and_unknown_types():
    cont = {
        "barrel": {
            "type": "Container",
            "id": "barrel",
            "container_flags": "ORGANIC | RESPAWNS",
            "encumbrance": 50.0,
            "inventory": [[3, "gold_001"], [1, "iron arrow"]],
        }
    }
    row = dict(
        zip(
            [c["label"] for c in table("CONT", [cont])["columns"]],
            table("CONT", [cont])["rows"]["barrel"],
            strict=True,
        )
    )
    assert (
        row["Organic"] == "yes"
        and row["Respawns"] == "yes"
        and row["Item list"] == "gold_001, iron arrow"
    )
    weap = {
        "w": {
            "type": "Weapon",
            "id": "w",
            "data": {
                "weapon_type": "ShortBladeOneHand",
                "chop_min": 1,
                "chop_max": 5,
                "flags": "SILVER",
            },
        }
    }
    wrow = dict(
        zip(
            [c["label"] for c in table("WEAP", [weap])["columns"]],
            table("WEAP", [weap])["rows"]["w"],
            strict=True,
        )
    )
    assert (
        wrow["Type"] == "Short Blade One Hand" and wrow["Chop"] == "1-5" and wrow["Silver"] == "yes"
    )
    assert table("GMST", []) == {"columns": [], "rows": {}}
