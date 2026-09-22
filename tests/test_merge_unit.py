"""Unit tests for :mod:`wraithguard.merge` covering paths the golden fixtures miss.

The golden tests exercise ``merge_plugins`` end to end; these cover the load-order
merge, texture and master remapping, moved/duplicate references, the deletion
reference-cleaning graph, ignored stripping, and record re-emission -- built from
small hand-made records so each branch is hit deliberately.
"""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from pathlib import Path

from wraithguard.esp.enums import DialogueType2, FileType
from wraithguard.esp.flags import CellFlags, ObjectFlags
from wraithguard.esp.plugin import read_plugin, write_plugin
from wraithguard.esp.records._ai import AiActivatePackage, AiFollowPackage, TravelDestination
from wraithguard.esp.records.activator import Activator
from wraithguard.esp.records.armor import Armor
from wraithguard.esp.records.bipedobject import BipedObject
from wraithguard.esp.records.birthsign import Birthsign
from wraithguard.esp.records.cell import Cell, CellData
from wraithguard.esp.records.container import Container
from wraithguard.esp.records.creature import Creature
from wraithguard.esp.records.dialogue import Dialogue
from wraithguard.esp.records.dialogueinfo import DialogueInfo
from wraithguard.esp.records.door import Door
from wraithguard.esp.records.enchanting import Enchanting
from wraithguard.esp.records.header import Header
from wraithguard.esp.records.landscape import Landscape
from wraithguard.esp.records.landscapetexture import LandscapeTexture
from wraithguard.esp.records.leveledcreature import LeveledCreature
from wraithguard.esp.records.leveleditem import LeveledItem
from wraithguard.esp.records.magiceffect import MagicEffect
from wraithguard.esp.records.npc import Npc
from wraithguard.esp.records.pathgrid import PathGrid, PathGridData
from wraithguard.esp.records.reference import Reference
from wraithguard.esp.records.region import Region
from wraithguard.esp.records.script import Script
from wraithguard.esp.records.skill import Skill
from wraithguard.esp.records.startscript import StartScript
from wraithguard.esp.records.static_ import Static
from wraithguard.esp.records.weapon import Weapon
from wraithguard.merge import (
    MergeOptions,
    PluginData,
    apply_moved_references,
    merge_load_order,
    merge_plugin_into,
    merge_plugins,
    remove_duplicate_references,
)
from wraithguard.merge.deletions import remove_deleted
from wraithguard.merge.ignored import remove_ignored, set_all_ignored
from wraithguard.merge.masters import (
    ensure_master_present,
    next_reference_index,
    remap_load_order_masters,
    remap_masters,
)
from wraithguard.merge.model import editor_id, object_key
from wraithguard.merge.textures import next_texture_index, remap_textures

_DELETED = ObjectFlags.DELETED


def _interior(
    name: str, references: list[Reference] | None = None, flags: ObjectFlags | None = None
) -> Cell:
    """An interior cell with the given references."""
    return Cell(
        flags=flags or ObjectFlags(0),
        name=name,
        data=CellData(cell_flags=CellFlags.IS_INTERIOR, grid=(0, 0)),
        references=references or [],
    )


def _exterior(coords: tuple[int, int], references: list[Reference] | None = None) -> Cell:
    """An exterior cell at ``coords`` with the given references."""
    return Cell(
        flags=ObjectFlags(0),
        name="",
        data=CellData(cell_flags=CellFlags(0), grid=coords),
        references=references or [],
    )


def _ref(id_: str, mast: int = 0, refr: int = 1, **kw: object) -> Reference:
    """A reference to ``id_``."""
    return Reference(mast_index=mast, refr_index=refr, id=id_, **kw)


# --------------------------------------------------------------------------- #
# model
# --------------------------------------------------------------------------- #


def test_editor_id_uses_numeric_index_for_skill_and_effect() -> None:
    """SKIL/MGEF have no id string, so their fixed index stands in."""
    assert editor_id(Skill()) == str(int(Skill().skill_id))
    assert editor_id(MagicEffect()) == str(int(MagicEffect().effect_id))


def test_object_key_is_none_for_non_objects() -> None:
    """The header (and cells/dialogue) are not ordinary objects."""
    assert object_key(Header()) is None
    assert object_key(Static(id="")) is None  # empty id
    assert object_key(Static(id="Rock"))[0] == b"\x00\x00\x00\x00"
    assert object_key(Skill())[0] == b"SKIL"


def test_into_records_round_trips_every_bucket() -> None:
    """Emitting and re-collecting preserves objects, cells and dialogue order."""
    records = [
        Header(),
        Static(id="rock"),
        _interior("Home", [_ref("rock")]),
        _exterior((1, 2)),
        Landscape(grid=(1, 2)),
        Dialogue(id="topic", dialogue_type=DialogueType2.Topic),
        DialogueInfo(id="i1"),
        Dialogue(id="quest", dialogue_type=DialogueType2.Journal),
        DialogueInfo(id="j1"),
    ]
    plugin = PluginData.from_records(records)
    assert plugin.count_objects() == len(records)

    emitted = plugin.into_records()
    # Journals must be emitted before topics.
    dial_ids = [r.id for r in emitted if isinstance(r, Dialogue)]
    assert dial_ids.index("quest") < dial_ids.index("topic")

    again = PluginData.from_records(emitted)
    assert set(again.objects) == set(plugin.objects)
    assert set(again.cells.interiors) == set(plugin.cells.interiors)
    assert set(again.cells.exteriors) == set(plugin.cells.exteriors)
    assert set(again.dialogues) == set(plugin.dialogues)


def test_collect_merges_duplicate_cell_and_redefined_topic() -> None:
    """Two records for the same cell union; a redefined topic reuses its group."""
    plugin = PluginData.from_records(
        [
            Header(),
            _interior("Home", [_ref("a", refr=1)]),
            _interior("Home", [_ref("b", refr=2)]),
            Dialogue(id="t", dialogue_type=DialogueType2.Topic),
            DialogueInfo(id="i1"),
            Dialogue(id="t", dialogue_type=DialogueType2.Topic),
            DialogueInfo(id="i2"),
        ]
    )
    assert {r.id for r in plugin.cells.interiors["home"].cell.references} == {"a", "b"}
    assert len(plugin.dialogues) == 1
    assert len(plugin.dialogues["t"].infos) == 2


def test_orphan_pathgrid_falls_back_to_interior_then_exterior() -> None:
    """A path grid whose cell is absent lands on an interior (0,0) or an exterior."""
    named = PluginData.from_records(
        [Header(), PathGrid(cell="Ghost", data=PathGridData(grid=(0, 0)))]
    )
    assert "ghost" in named.cells.interiors
    placed = PluginData.from_records([Header(), PathGrid(cell="", data=PathGridData(grid=(3, 4)))])
    assert (3, 4) in placed.cells.exteriors


# --------------------------------------------------------------------------- #
# combine
# --------------------------------------------------------------------------- #


def test_merge_plugin_into_unions_and_replaces() -> None:
    """Objects are replaced, cell references unioned, landscape/pathgrid kept latest."""
    base = PluginData.from_records(
        [
            Header(),
            Static(id="rock", mesh="old.nif"),
            _interior("Home", [_ref("rock", refr=1)]),
            _exterior((0, 0)),
            Landscape(grid=(0, 0)),
        ]
    )
    incoming = PluginData.from_records(
        [
            Header(),
            Static(id="rock", mesh="new.nif"),
            _interior("Home", [_ref("tree", refr=2)]),
            _exterior((0, 0)),
            Landscape(grid=(0, 0), landscape_flags=Landscape().landscape_flags),
            PathGrid(cell="", data=PathGridData(grid=(0, 0))),
        ]
    )
    merge_plugin_into(incoming, base)

    assert base.objects[(b"\x00\x00\x00\x00", "rock")].mesh == "new.nif"
    home = base.cells.interiors["home"].cell
    assert {r.id for r in home.references} == {"rock", "tree"}
    assert base.cells.exteriors[(0, 0)].pathgrid is not None


def test_merge_dialogue_group_splices_and_relinks() -> None:
    """Merging responses keeps order and repairs prev/next links."""
    base = PluginData.from_records(
        [Header(), Dialogue(id="t", dialogue_type=DialogueType2.Topic), DialogueInfo(id="a")]
    )
    incoming = PluginData.from_records(
        [
            Header(),
            Dialogue(id="t", dialogue_type=DialogueType2.Topic),
            DialogueInfo(id="b", prev_id="a"),
        ]
    )
    merge_plugin_into(incoming, base)
    infos = base.dialogues["t"].infos
    assert [i.id for i in infos] == ["a", "b"]
    assert infos[0].next_id == "b" and infos[1].prev_id == "a"


# --------------------------------------------------------------------------- #
# masters
# --------------------------------------------------------------------------- #


def test_ensure_master_present_appends_when_absent() -> None:
    """A target not in the list is appended."""
    masters: list[tuple[str, int]] = [("Morrowind.esm", 1)]
    ensure_master_present(masters, "Master.esm", 99)
    assert masters[-1] == ("Master.esm", 99)


def test_ensure_master_present_rejects_non_last_target() -> None:
    """A target present but not last is an error."""
    masters = [("Master.esm", 1), ("Other.esm", 2)]
    with pytest.raises(ValueError, match="last master"):
        ensure_master_present(masters, "Master.esm", 1)


def test_remap_masters_adopts_header_and_renumbers_local_refs() -> None:
    """A local reference is renumbered above the master's highest local index."""
    master = PluginData.from_records([Header(), _interior("Home", [_ref("x", mast=0, refr=7)])])
    master.header.author = "cartographer"
    plugin = PluginData()
    plugin.header = Header(masters=[("Master.esm", 0)])
    plugin.cells.interiors["home"] = master.cells.interiors["home"].__class__(
        cell=_interior("Home", [_ref("y", mast=0, refr=1)])
    )
    assert next_reference_index(master) == 8
    remap_masters(plugin, master, "Master.esm")
    assert plugin.header.author == "cartographer"
    moved = plugin.cells.interiors["home"].cell.references[0]
    assert moved.refr_index == 8  # renumbered from the master's next free index


def test_remap_masters_appends_new_master() -> None:
    """A plugin master absent from the target is added to the merged master list."""
    master = PluginData()
    master.header = Header(masters=[])
    plugin = PluginData()
    plugin.header = Header(masters=[("Extra.esm", 0), ("Master.esm", 0)])
    remap_masters(plugin, master, "Master.esm")
    assert [name for name, _ in plugin.header.masters] == ["Extra.esm"]


def test_remap_load_order_rejects_unknown_master() -> None:
    """A plugin master missing from the merged remap is an error."""
    from wraithguard.merge.masters import remap_load_order_masters

    plugin = PluginData()
    plugin.header = Header(masters=[("Ghost.esm", 0)])
    with pytest.raises(ValueError, match="not found"):
        remap_load_order_masters(plugin, 0, {})


# --------------------------------------------------------------------------- #
# textures
# --------------------------------------------------------------------------- #


def _land_with_index(grid: tuple[int, int], stored_value: int) -> Landscape:
    """An exterior landscape whose first painted texture cell is ``stored_value``."""
    values = [0] * 256
    values[0] = stored_value
    return Landscape(grid=grid, texture_indices=struct.pack("<256H", *values))


def test_remap_textures_rewrites_indices_and_painted_grid() -> None:
    """A texture index is remapped to the master's, and the painted grid follows."""
    plugin = PluginData.from_records(
        [Header(), LandscapeTexture(id="tx", index=5), _exterior((0, 0))]
    )
    plugin.cells.exteriors[(0, 0)].landscape = _land_with_index((0, 0), 6)  # 5 + 1
    master = PluginData.from_records([Header(), LandscapeTexture(id="tx", index=2)])

    remap_textures(plugin, master)

    assert plugin.objects[(b"LTEX", "tx")].index == 2
    grid = struct.unpack("<256H", plugin.cells.exteriors[(0, 0)].landscape.texture_indices)
    assert grid[0] == 3  # 2 + 1


def test_remap_textures_noop_when_master_has_no_textures() -> None:
    """With no textures in the master there is nothing to remap."""
    master = PluginData.from_records([Header()])
    assert next_texture_index(master) is None
    plugin = PluginData.from_records([Header(), LandscapeTexture(id="tx", index=5)])
    remap_textures(plugin, master)  # must not raise
    assert plugin.objects[(b"LTEX", "tx")].index == 5


def test_remap_textures_assigns_next_index_for_unknown_texture() -> None:
    """A texture absent from the master gets the next free index; matches are left."""
    plugin = PluginData.from_records(
        [
            Header(),
            LandscapeTexture(id="a", index=5),
            LandscapeTexture(id="b", index=9),
            _exterior((0, 0)),
        ]
    )
    plugin.cells.exteriors[(0, 0)].landscape = _land_with_index(
        (0, 0), 10
    )  # b's grid value (9 + 1)
    plugin.cells.exteriors[(1, 0)] = plugin.cells.get_or_create_exterior((1, 0))  # no landscape
    master = PluginData.from_records([Header(), LandscapeTexture(id="a", index=5)])

    remap_textures(plugin, master)
    assert plugin.objects[(b"LTEX", "a")].index == 5  # unchanged (matched)
    assert plugin.objects[(b"LTEX", "b")].index == 6  # next free index in master
    grid = struct.unpack("<256H", plugin.cells.exteriors[(0, 0)].landscape.texture_indices)
    assert grid[0] == 7  # 6 + 1


# --------------------------------------------------------------------------- #
# deletions
# --------------------------------------------------------------------------- #


def test_remove_deleted_cleans_the_reference_graph() -> None:
    """Deleting objects clears every field and list entry that named them."""
    from wraithguard.esp.records.bodypart import Bodypart as _Bodypart
    from wraithguard.esp.records.class_ import Class as _Class
    from wraithguard.esp.records.faction import Faction as _Faction
    from wraithguard.esp.records.sound import Sound as _Sound
    from wraithguard.esp.records.spell import Spell as _Spell

    records = [
        Header(),
        Static(id="rock", flags=_DELETED),
        Script(id="scr", flags=_DELETED),
        Enchanting(id="ench", flags=_DELETED),
        _Spell(id="sp", flags=_DELETED),
        _Sound(id="snd", flags=_DELETED),
        _Faction(id="fac", flags=_DELETED),
        _Class(id="cls", flags=_DELETED),
        _Bodypart(id="bp", flags=_DELETED),
        Door(id="door", script="scr", open_sound="snd", close_sound="snd"),
        Weapon(id="wp", script="scr", enchanting="ench"),
        Container(id="cont", script="scr", inventory=[(1, "rock")]),
        Npc(
            id="npc",
            script="scr",
            inventory=[(1, "rock")],
            spells=["sp"],
            class_="cls",
            faction="fac",
            head="bp",
            hair="bp",
            ai_packages=[AiFollowPackage(target="rock", cell="DelInt")],
            travel_destinations=[TravelDestination(cell="DelInt")],
        ),
        Creature(
            id="cr",
            script="scr",
            inventory=[(1, "rock")],
            spells=["sp"],
            ai_packages=[AiActivatePackage(target="rock")],
        ),
        Armor(
            id="arm",
            enchanting="ench",
            biped_objects=[BipedObject(male_bodypart="bp", female_bodypart="bp")],
        ),
        Region(id="reg", sleep_creature="rock", sounds=[("snd", 10)]),
        MagicEffect(cast_sound="snd", cast_visual="rock"),
        LeveledItem(id="lev", items=[("rock", 1)]),
        LeveledCreature(id="levc", creatures=[("rock", 1)]),
        Birthsign(id="bs", spells=["sp"]),
        StartScript(id="ss", script="scr"),
        Activator(id="act", script="scr"),
        _interior("DelInt", flags=_DELETED),
        _interior(
            "Live", [_ref("rock", refr=1), _ref("gone", refr=2, deleted=True), _ref("keep", refr=3)]
        ),
    ]
    plugin = PluginData.from_records(records)
    remove_deleted(plugin)

    door = plugin.objects[(b"\x00\x00\x00\x00", "door")]
    assert door.script == "" and door.open_sound == "" and door.close_sound == ""
    assert plugin.objects[(b"\x00\x00\x00\x00", "wp")].enchanting == ""
    assert plugin.objects[(b"\x00\x00\x00\x00", "cont")].inventory == []
    npc = plugin.objects[(b"\x00\x00\x00\x00", "npc")]
    assert npc.inventory == [] and npc.spells == [] and npc.class_ == "" and npc.faction == ""
    assert npc.head == "" and npc.hair == ""
    assert npc.ai_packages[0].target == "" and npc.ai_packages[0].cell == ""
    assert npc.travel_destinations[0].cell == ""
    cr = plugin.objects[(b"\x00\x00\x00\x00", "cr")]
    assert cr.spells == [] and cr.ai_packages[0].target == ""
    arm = plugin.objects[(b"\x00\x00\x00\x00", "arm")]
    assert arm.enchanting == "" and arm.biped_objects[0].male_bodypart == ""
    reg = plugin.objects[(b"REGN", "reg")]
    assert reg.sleep_creature == "" and reg.sounds == []
    mgef = plugin.objects[(b"MGEF", editor_id(MagicEffect()))]
    assert mgef.cast_sound == "" and mgef.cast_visual == ""
    assert plugin.objects[(b"\x00\x00\x00\x00", "lev")].items == []
    assert plugin.objects[(b"\x00\x00\x00\x00", "levc")].creatures == []
    assert plugin.objects[(b"BSGN", "bs")].spells == []
    assert plugin.objects[(b"SSCR", "ss")].script == ""
    assert plugin.objects[(b"\x00\x00\x00\x00", "act")].script == ""
    # Deleted objects and the deleted interior are gone.
    assert (b"\x00\x00\x00\x00", "rock") not in plugin.objects
    assert "delint" not in plugin.cells.interiors
    # Live cell keeps only the surviving reference.
    assert [r.id for r in plugin.cells.interiors["live"].cell.references] == ["keep"]


def test_remove_deleted_clears_cell_region() -> None:
    """A cell whose region was deleted loses the region link."""
    from wraithguard.esp.records.region import Region as _Region

    home = _interior("Home")
    home.region = "reg"
    plugin = PluginData.from_records([Header(), _Region(id="reg", flags=_DELETED), home])
    remove_deleted(plugin)
    assert plugin.cells.interiors["home"].cell.region is None


def test_remove_deleted_mends_dialogue_ends() -> None:
    """Deleting the first and last responses clears the dangling end links."""
    records = [
        Header(),
        Dialogue(id="t", dialogue_type=DialogueType2.Topic),
        DialogueInfo(id="a", flags=_DELETED),
        DialogueInfo(id="b", prev_id="a"),
        DialogueInfo(id="c", prev_id="b", flags=_DELETED),
        Dialogue(id="dead", dialogue_type=DialogueType2.Topic, flags=_DELETED),
    ]
    plugin = PluginData.from_records(records)
    remove_deleted(plugin)
    assert "dead" not in plugin.dialogues
    infos = plugin.dialogues["t"].infos
    assert [i.id for i in infos] == ["b"]
    assert infos[0].prev_id == "" and infos[0].next_id == ""


# --------------------------------------------------------------------------- #
# ignored
# --------------------------------------------------------------------------- #


def test_set_all_ignored_then_remove_strips_everything() -> None:
    """Flagging all ignored and stripping leaves an empty plugin."""
    plugin = PluginData.from_records(
        [
            Header(),
            Static(id="rock"),
            _interior("Home"),
            _exterior((0, 0)),
            Landscape(grid=(0, 0)),
            Dialogue(id="t", dialogue_type=DialogueType2.Topic),
            DialogueInfo(id="a"),
        ]
    )
    set_all_ignored(plugin, True)
    remove_ignored(plugin)
    assert not plugin.objects
    assert not plugin.cells.interiors and not plugin.cells.exteriors
    assert not plugin.dialogues


# --------------------------------------------------------------------------- #
# api
# --------------------------------------------------------------------------- #


def test_apply_moved_references_moves_into_destination() -> None:
    """A local moved reference is placed into its destination cell."""
    plugin = PluginData()
    plugin.cells.exteriors[(0, 0)] = plugin.cells.get_or_create_exterior((0, 0))
    plugin.cells.exteriors[(0, 0)].cell = _exterior((0, 0), [_ref("wanderer", moved_cell=(1, 0))])
    plugin.cells.exteriors[(1, 0)] = plugin.cells.get_or_create_exterior((1, 0))
    plugin.cells.exteriors[(1, 0)].cell = _exterior((1, 0))

    apply_moved_references(plugin)
    assert plugin.cells.exteriors[(0, 0)].cell.references == []
    dest = plugin.cells.exteriors[(1, 0)].cell.references
    assert len(dest) == 1 and dest[0].moved_cell is None


def test_apply_moved_references_rejects_missing_destination() -> None:
    """A moved reference to an absent cell is an error."""
    plugin = PluginData()
    plugin.cells.get_or_create_exterior((0, 0)).cell = _exterior(
        (0, 0), [_ref("lost", moved_cell=(9, 9))]
    )
    with pytest.raises(ValueError, match="invalid cell"):
        apply_moved_references(plugin)


def test_remove_duplicate_references_keeps_one() -> None:
    """Identical id-and-transform references collapse to one; distinct ones stay."""
    plugin = PluginData()
    cell = _interior(
        "Home",
        [
            _ref("torch", refr=1, translation=(1.0, 2.0, 3.0)),
            _ref("torch", refr=2, translation=(1.0, 2.0, 3.0)),
            _ref("torch", refr=3, translation=(9.0, 9.0, 9.0)),
        ],
    )
    plugin.cells.get_or_create_interior("Home").cell = cell
    remove_duplicate_references(plugin)
    kept = plugin.cells.interiors["home"].cell.references
    assert sorted(r.refr_index for r in kept) == [2, 3]


def test_merge_options_apply_runs_all_passes() -> None:
    """Every enabled pass runs without error."""
    plugin = PluginData.from_records([Header(), Static(id="rock", flags=_DELETED)])
    MergeOptions(remove_deleted=True, apply_moved_references=True).apply(plugin)
    assert (b"\x00\x00\x00\x00", "rock") not in plugin.objects


def test_merge_load_order_combines_plugins(tmp_path: Path) -> None:
    """Merging a load order lists every plugin as a master and remaps local refs."""
    a = tmp_path / "A.esp"
    b = tmp_path / "B.esp"
    a.write_bytes(write_plugin([Header(), Static(id="rock")]))
    b.write_bytes(
        write_plugin(
            [
                Header(),
                Static(id="tree"),
                _interior("Home", [_ref("tree", mast=0, refr=1)]),
            ]
        )
    )
    merged = merge_load_order([a, b], MergeOptions())
    assert merged.header.file_type == FileType.Esm
    assert [name for name, _ in merged.header.masters] == ["A.esp", "B.esp"]
    assert (b"\x00\x00\x00\x00", "rock") in merged.objects
    assert (b"\x00\x00\x00\x00", "tree") in merged.objects
    # B is load-order index 1, so its local references now point at master index 2.
    ref = merged.cells.interiors["home"].cell.references[0]
    assert ref.mast_index == 2


def test_merge_plugins_rejects_non_last_target(tmp_path: Path) -> None:
    """merge_plugins refuses a plugin whose target master is not listed last."""
    plugin = tmp_path / "Plugin.esp"
    master = tmp_path / "Master.esm"
    master.write_bytes(write_plugin([Header(file_type=FileType.Esm)]))
    plugin.write_bytes(
        write_plugin([Header(masters=[("Master.esm", master.stat().st_size), ("Other.esm", 1)])])
    )
    with pytest.raises(ValueError, match="last master"):
        merge_plugins(plugin, master, MergeOptions())


def test_merge_plugins_matches_read_plugin_helpers(tmp_path: Path) -> None:
    """A plugin with no extra masters merges into its master and gains its objects."""
    master = tmp_path / "Master.esm"
    plugin = tmp_path / "Plugin.esp"
    master.write_bytes(
        write_plugin([Header(file_type=FileType.Esm), Static(id="rock", mesh="a.nif")])
    )
    plugin.write_bytes(
        write_plugin(
            [
                Header(masters=[("Master.esm", master.stat().st_size)]),
                Static(id="rock", mesh="b.nif"),
                Static(id="tree"),
            ]
        )
    )
    merged = merge_plugins(plugin, master, MergeOptions())
    assert merged.objects[(b"\x00\x00\x00\x00", "rock")].mesh == "b.nif"
    assert (b"\x00\x00\x00\x00", "tree") in merged.objects
    # Sanity: the result re-serialises and re-parses.
    assert read_plugin(write_plugin(merged.into_records()))


def test_collect_dedups_duplicate_keyed_references_keeping_last() -> None:
    """A cell listing the same (mast, refr) twice keeps only the last, as tes3 does.

    tes3 stores references in a map keyed by ``(mast_index, refr_index)``, so a
    malformed cell that lists one object index twice collapses on load. Our
    list-based reader reproduces that collapse when the cell is first bucketed.
    """
    cell = _interior(
        "Dup",
        [
            _ref("old", mast=0, refr=7),
            _ref("new", mast=0, refr=7),  # same key: wins
            _ref("other", mast=0, refr=8),
        ],
    )
    plugin = PluginData.from_records([Header(), cell])
    kept = plugin.cells.interiors["dup"].cell.references
    keys = {(r.mast_index, r.refr_index) for r in kept}
    assert keys == {(0, 7), (0, 8)}
    winner = next(r for r in kept if (r.mast_index, r.refr_index) == (0, 7))
    assert winner.id == "new"


def test_load_order_merge_drops_out_of_range_reference() -> None:
    """A reference citing a master the plugin doesn't declare is dropped, not fatal.

    merge_to_master's ``from_path_remap_masters`` filters such references out; a
    dirty plugin must not abort the whole load-order merge.
    """
    good = _ref("keep", mast=0, refr=1)
    dirty = _ref("gone", mast=5, refr=2)  # no 5th master in the header
    plugin = PluginData.from_records([Header(), _exterior((0, 0), [good, dirty])])
    remap_load_order_masters(plugin, 0, {})
    kept = plugin.cells.exteriors[(0, 0)].cell.references
    assert [r.id for r in kept] == ["keep"]
    assert kept[0].mast_index == 1  # local ref now points at this plugin (index 0 + 1)
