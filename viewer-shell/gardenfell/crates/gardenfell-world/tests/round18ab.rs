//! Round 18ab — what counts as a plugin in a load order.
//!
//! Robin: "I didn't find anything by switching to my openmw.cfg file (after MO2 was
//! already loaded on start up of the program). You might want to analyze it and see if
//! that is to be expected or not, but I think I should at least find something (the
//! vanilla game) or so."
//!
//! His file holds 912 `data=` lines and exactly one `content=` line, and that one names
//! `talkingMatters.omwscripts` — a list of Lua scripts, which has no records in it. The
//! load order was therefore "not empty" by a count of one, `World::load` opened a file
//! with nothing to open, and the program connected to a world with no cells rather than
//! saying what was wrong. An OpenMW setup is two config files and only the user's one
//! lists the game; his was the installation's.

use gardenfell_world::world::read_load_order;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gdn-18ab-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn cfg(dir: &PathBuf, text: &str) -> PathBuf {
    let p = dir.join("openmw.cfg");
    std::fs::write(&p, text).unwrap();
    p
}

#[test]
fn a_script_list_is_not_a_plugin() {
    let d = tmp("scripts");
    // Robin's file, in miniature: the data directories, and one content line that is not
    // a game file.
    let p = cfg(
        &d,
        "data=\"C:/Games/Morrowind/Data Files\"\n\
         data-local=\"C:/Users/r/Documents/My Games/OpenMW/data\"\n\
         content=talkingMatters.omwscripts\n",
    );
    assert!(
        read_load_order(&p).is_empty(),
        "an .omwscripts entry has no records and must not stand in for a load order"
    );
}

#[test]
fn the_four_kinds_of_game_file_all_count() {
    let d = tmp("kinds");
    let p = cfg(
        &d,
        "content=Morrowind.esm\ncontent=Tribunal.ESM\ncontent=Bloodmoon.esp\n\
         content=example.omwgame\ncontent=example.omwaddon\n\
         content=lua_thing.omwscripts\n",
    );
    assert_eq!(
        read_load_order(&p),
        vec![
            "Morrowind.esm".to_string(),
            "Tribunal.ESM".to_string(),
            "Bloodmoon.esp".to_string(),
            "example.omwgame".to_string(),
            "example.omwaddon".to_string()
        ],
        "case is the file system's business, and the script list is still not a plugin"
    );
}

#[test]
fn the_order_content_is_written_in_is_the_order_it_loads_in() {
    // Unlike `GameFileN=`, `content=` carries no index: the file's own order is the load
    // order, and dropping a line out of the middle must not disturb the rest.
    let d = tmp("order");
    let p = cfg(
        &d,
        "content=Morrowind.esm\ncontent=lua.omwscripts\ncontent=Tribunal.esm\n\
         content=Mod.esp\n",
    );
    assert_eq!(
        read_load_order(&p),
        vec!["Morrowind.esm".to_string(), "Tribunal.esm".to_string(), "Mod.esp".to_string()]
    );
}

#[test]
fn a_morrowind_ini_is_still_read_by_index() {
    let d = tmp("ini");
    let p = d.join("Morrowind.ini");
    std::fs::write(
        &p,
        "[Game Files]\nGameFile2=Bloodmoon.esm\nGameFile0=Morrowind.esm\nGameFile1=Tribunal.esm\n",
    )
    .unwrap();
    assert_eq!(
        read_load_order(&p),
        vec!["Morrowind.esm".to_string(), "Tribunal.esm".to_string(), "Bloodmoon.esm".to_string()],
        "GameFileN is numbered, and the number is the order"
    );
}
