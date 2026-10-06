"""wraithguard.patch.dialogue_graph: flows, maps, flags and the rehearsed dialogue window."""

from __future__ import annotations

import pytest

from wraithguard.patch.dialogue_graph import DialogueIndex, npc_of, script_effects


def _dial(tid: str, kind: str = "Topic") -> dict:
    return {"type": "Dialogue", "id": tid, "dialogue_type": kind}


def _info(iid: str, prev: str = "", text: str = "", script: str = "", **kw) -> dict:
    rec = {"type": "DialogueInfo", "id": iid, "prev_id": prev, "text": text, "script_text": script}
    rec.update(kw)
    return rec


def _choice(n: int) -> dict:
    return {
        "filter_type": "Function",
        "function": "Choice",
        "comparison": "Equal",
        "value": {"type": "Integer", "data": n},
    }


MASTER = [
    _dial("Greeting 1", "Greeting"),
    _info("g1", text="Hello, %PCName. Ask me about my little secret.", speaker_id="fargoth"),
    _info("g2", "g1", text="What do you want?"),
    _dial("little secret"),
    _info("s1", text="Will you help me?", script='Choice "Yes" 1 "No" 2', speaker_id="fargoth"),
    _info(
        "s2",
        "s1",
        text="Thank you!",
        script='Journal "MS_Lookout" 10\nset FargothHelped to 1\nAddTopic "hiding place"',
        speaker_id="fargoth",
        filters=[_choice(1)],
    ),
    _info("s3", "s2", text="Pity.", script="Goodbye", speaker_id="fargoth", filters=[_choice(2)]),
    _info("s4", "s3", text="Leave me be.", speaker_race="Wood Elf"),
    _dial("hiding place"),
    _info(
        "h1",
        text="The stump.",
        filters=[
            {
                "filter_type": "Global",
                "id": "FargothHelped",
                "comparison": "Equal",
                "value": {"type": "Integer", "data": 1},
            }
        ],
    ),
    _dial("background"),
    _info("b1", text="I am nobody. Do not ask about my Little Secret!"),
]


@pytest.fixture
def index() -> DialogueIndex:
    return DialogueIndex([("Morrowind.esm", MASTER)])


def test_script_effects():
    eff = script_effects(
        'Choice "A" 1, "B" 2 ; pick\nplayer->AddItem "gold_001" 5\nset x to 2\nfoo->Disable\nGoodbye'
    )
    assert eff.choices == [("A", 1), ("B", 2)] and eff.items == [("gold_001", "add")]
    assert eff.sets == [("x", "")] and eff.goodbye and eff.other == ["foo->Disable"]


def test_flow_and_links(index):
    flow = index.flow("LITTLE SECRET")
    rows = flow["responses"]
    assert [r["id"] for r in rows] == ["s1", "s2", "s3", "s4"]
    assert rows[0]["choices"] == [{"label": "Yes", "n": 1}, {"label": "No", "n": 2}]
    assert rows[1]["gate"] == 1 and rows[1]["journal"] == [{"quest": "MS_Lookout", "index": 10}]
    assert rows[1]["addtopics"] == ["hiding place"] and rows[2]["goodbye"]
    assert {e["from"] for e in flow["into"]} == {"background", "Greeting 1"}
    segs = index.segments("Do not ask about my Little Secret!")
    assert {"t": "Little Secret", "topic": "little secret"} in segs
    assert index.segments("%PCName")[0] == {"t": "%PCName", "macro": "PCName"}


def test_map_and_flags(index):
    m = index.neighbourhood("little secret")
    ids = {n["id"] for n in m["nodes"]}
    assert {
        "t:little secret",
        "t:hiding place",
        "t:background",
        "q:ms_lookout",
        "v:fargothhelped",
    } <= ids
    # The flag between trees: set in one, tested in another.
    assert {"from": "v:fargothhelped", "to": "t:hiding place", "kind": "reads"} in m["edges"]
    flags = {(f["kind"], f["name"].lower()): f for f in index.flags()["flags"]}
    assert flags[("variable", "fargothhelped")]["writers"] == 1
    assert flags[("variable", "fargothhelped")]["readers"] == 1
    one = index.flags("FargothHelped")
    assert one["readers"][0]["topic"] == "hiding place"


def test_rehearsal(index):
    fargoth = npc_of({"id": "fargoth", "name": "Fargoth", "race": "Wood Elf", "npc_flags": ""})
    got = index.play(fargoth)
    assert got["line"]["id"] == "g1"
    assert any(s.get("topic") == "little secret" for s in got["line"]["segments"])
    assert "little secret" in got["topics"]
    ask = index.play(fargoth, "little secret")
    assert ask["line"]["id"] == "s1" and len(ask["line"]["choices"]) == 2
    assert index.play(fargoth, "little secret", choice=2)["line"]["goodbye"]
    other = npc_of({"id": "someone", "race": "Wood Elf"})
    assert index.play(other, "little secret")["line"]["id"] == "s4"
    assert index.play(other)["line"]["id"] == "g2"
    with pytest.raises(ValueError):
        index.flow("nothing")


def test_pool_edits_are_said(index):
    edited = DialogueIndex([("Morrowind.esm", MASTER)], edits={"g1": {"text": "Changed."}})
    fargoth = npc_of({"id": "fargoth"})
    assert edited.play(fargoth)["line"]["segments"] == [{"t": "Changed."}]
