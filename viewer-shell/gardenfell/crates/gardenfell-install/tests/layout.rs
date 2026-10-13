//! Finding the load order from what the person pointed at.
//!
//! Round 18e, for the Linux packages: a filesystem that cares about case is the one
//! thing the plain-install path had never met. Everything inside the overlay was
//! already matched through the lowercase index; the ini beside `Data Files` was a plain
//! join, and `morrowind.ini` is a real thing to find in a folder that came through
//! Proton or a mod manager.

use gardenfell_install::layout;
use std::path::{Path, PathBuf};

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Dir {
        let mut p = std::env::temp_dir();
        p.push(format!("gf_layout_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const INI: &str = "[Game Files]\nGameFile0=Morrowind.esm\n";

#[test]
fn a_plain_install_reads_the_ini_beside_data_files() {
    let d = Dir::new("plain");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    std::fs::write(d.path().join("Morrowind.ini"), INI).unwrap();
    let l = layout::vanilla(&df).expect("a folder with its ini beside it");
    assert_eq!(l.order_file, d.path().join("Morrowind.ini"));
    assert_eq!(l.roots, vec![df.clone()]);
    assert_eq!(l.game_dir.as_deref(), Some(d.path()));
}

#[test]
fn the_ini_is_found_whatever_case_the_disk_gives_it() {
    let d = Dir::new("case");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    std::fs::write(d.path().join("MORROWIND.INI"), INI).unwrap();
    let l = layout::vanilla(&df).expect("found under another case");
    let name = l.order_file.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(name.to_ascii_lowercase(), "morrowind.ini", "got {}", name);
    assert!(std::fs::read_to_string(&l.order_file).unwrap().contains("GameFile0"));
}

#[test]
fn no_ini_at_all_is_said_plainly() {
    let d = Dir::new("none");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    let e = layout::vanilla(&df).unwrap_err();
    assert!(e.contains("no Morrowind.ini"), "{}", e);
}

// ---------------------------------------------------------------------------------------
// Round 18bh — `--install`, and working out what kind of install a path is
// ---------------------------------------------------------------------------------------
//
// The window asks which kind you have; the command line does not, and Robin asked that it
// not assume anything set up elsewhere: "as a user, for CLI purposes, it would be good to
// always ask for them in the commands when needed, rather than assume any paths based on
// what I set up in the visual GUI". So one `--install` has to be enough, and `detect` is
// what makes it enough. Everything it decides on is a file or a folder that is there — it
// never guesses from a name — because the cost of guessing wrong is a plugin generated
// against the wrong load order, which looks exactly like a plugin generated against the
// right one until you load it.

fn mo2_at(base: &Path, profile: &str) {
    std::fs::create_dir_all(base.join("mods/SomeMod/meshes")).unwrap();
    std::fs::create_dir_all(base.join(format!("profiles/{profile}"))).unwrap();
    std::fs::write(base.join("ModOrganizer.ini"), "[General]\nselected_profile=@ByteArray(Default)\n").unwrap();
    std::fs::write(base.join(format!("profiles/{profile}/modlist.txt")), "+SomeMod\n").unwrap();
    std::fs::write(base.join(format!("profiles/{profile}/plugins.txt")), "*Morrowind.esm\n").unwrap();
    // MO2 keeps the game's own ini per profile, and for Morrowind that file — not
    // plugins.txt — is where the plugin load order actually lives.
    std::fs::write(base.join(format!("profiles/{profile}/Morrowind.ini")), INI).unwrap();
}

#[test]
fn a_cfg_file_is_an_openmw_install_because_that_is_how_openmw_finds_it() {
    let d = Dir::new("det_cfg");
    let cfg = d.path().join("openmw.cfg");
    std::fs::create_dir_all(d.path().join("data")).unwrap();
    std::fs::write(&cfg, format!("data=\"{}\"\ncontent=Morrowind.esm\n", d.path().join("data").display())).unwrap();
    let l = layout::detect(&cfg, None).expect("a .cfg is an openmw config");
    assert!(l.label.starts_with("OpenMW"), "got {}", l.label);
}

#[test]
fn a_folder_holding_an_openmw_cfg_is_the_same_answer() {
    // Somebody points at the folder rather than the file inside it, which is the more
    // natural thing to do and costs one join to accept.
    let d = Dir::new("det_cfgdir");
    std::fs::create_dir_all(d.path().join("data")).unwrap();
    std::fs::write(
        d.path().join("openmw.cfg"),
        format!("data=\"{}\"\ncontent=Morrowind.esm\n", d.path().join("data").display()),
    )
    .unwrap();
    let l = layout::detect(d.path(), None).expect("the folder its config sits in");
    assert!(l.label.starts_with("OpenMW"), "got {}", l.label);
}

#[test]
fn a_modorganizer_ini_makes_it_an_mo2_install() {
    let d = Dir::new("det_mo2");
    mo2_at(d.path(), "Default");
    let l = layout::detect(d.path(), None).expect("an MO2 base folder");
    assert!(l.label.starts_with("MO2"), "got {}", l.label);
    assert_eq!(l.mo2_base.as_deref(), Some(d.path()));
}

#[test]
fn the_named_mo2_profile_is_the_one_read() {
    let d = Dir::new("det_mo2p");
    mo2_at(d.path(), "Default");
    std::fs::create_dir_all(d.path().join("profiles/Grass")).unwrap();
    std::fs::write(d.path().join("profiles/Grass/modlist.txt"), "-SomeMod\n").unwrap();
    std::fs::write(d.path().join("profiles/Grass/plugins.txt"), "*Morrowind.esm\n").unwrap();
    std::fs::write(d.path().join("profiles/Grass/Morrowind.ini"), INI).unwrap();
    let on = layout::detect(d.path(), Some("Default")).expect("the default profile");
    let off = layout::detect(d.path(), Some("Grass")).expect("the named profile");
    assert_eq!(on.mods_on, 1, "SomeMod is switched on in Default");
    assert_eq!(off.mods_on, 0, "and off in Grass — which is the whole point of profiles");
}

#[test]
fn a_game_folder_is_accepted_in_place_of_its_data_files() {
    // The easy mistake, and the fix is one join: point at the game, get its Data Files.
    let d = Dir::new("det_game");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    std::fs::write(d.path().join("Morrowind.ini"), INI).unwrap();
    let l = layout::detect(d.path(), None).expect("a game folder");
    assert_eq!(l.roots, vec![df]);
    assert_eq!(l.game_dir.as_deref(), Some(d.path()));
}

#[test]
fn a_data_files_folder_pointed_at_directly_is_taken_at_its_word() {
    let d = Dir::new("det_df");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    std::fs::write(d.path().join("Morrowind.ini"), INI).unwrap();
    let l = layout::detect(&df, None).expect("the Data Files folder itself");
    assert_eq!(l.roots, vec![df]);
}

#[test]
fn a_path_that_is_not_there_is_refused_rather_than_guessed_at() {
    let d = Dir::new("det_gone");
    let missing = d.path().join("no-such-folder");
    assert!(layout::detect(&missing, None).is_err());
}

#[test]
fn a_file_that_is_not_a_config_is_refused() {
    // A plugin, an ini, a readme: none of them is an install, and saying so beats reading
    // the folder it happens to sit in.
    let d = Dir::new("det_file");
    let f = d.path().join("Morrowind.esm");
    std::fs::write(&f, b"TES3").unwrap();
    assert!(layout::detect(&f, None).is_err());
}

#[test]
fn mo2_is_decided_before_openmw_because_a_base_folder_can_hold_both() {
    // MO2 can manage an OpenMW install, so its base may have an openmw.cfg in it. The
    // ModOrganizer.ini is the more specific fact and wins.
    let d = Dir::new("det_both");
    mo2_at(d.path(), "Default");
    std::fs::write(d.path().join("openmw.cfg"), "content=Morrowind.esm\n").unwrap();
    let l = layout::detect(d.path(), None).expect("both markers present");
    assert!(l.label.starts_with("MO2"), "got {}", l.label);
}

/* ---- Round 18bs: the overlay's own direction, which nothing pinned ---------------- */

#[test]
fn openmw_data_dirs_go_into_the_overlay_last_listed_first() {
    /* `layout::openmw` reverses the `data=` list, and that is right: OpenMW registers the
       data directories in listing order into a VFS where the last one added wins — its own
       source says so twice, `// Last data dir has the highest priority` beside
       `// Last BSA has the highest priority`. So the last line in the file is the highest
       priority and belongs first in an overlay that resolves first-match-wins.
       Nothing pinned it. Every layout test until now used a single `data=` line, so the
       reversal was never exercised — which is the same hole that let J2 live for as long as
       it did: the archive side of exactly this reasoning was backwards, and its fixture
       could not see it. A direction that is correct and untested is one edit from being
       incorrect and untested. */
    let d = Dir::new("omw_order");
    for n in ["vanilla", "patch", "replacer"] {
        std::fs::create_dir_all(d.path().join(n)).unwrap();
    }
    let cfg = d.path().join("openmw.cfg");
    std::fs::write(
        &cfg,
        format!(
            "data=\"{}\"\ndata=\"{}\"\ndata-local=\"{}\"\ncontent=Morrowind.esm\n",
            d.path().join("vanilla").display(),
            d.path().join("patch").display(),
            d.path().join("replacer").display(),
        ),
    )
    .unwrap();
    let l = layout::openmw(&cfg).expect("reads");
    let names: Vec<String> = l
        .roots
        .iter()
        .map(|r| r.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["replacer", "patch", "vanilla"], "last listed is highest priority");

    /* And the whole point of the ordering, through the thing that consumes it: the replacer
       wins the file all three supply, and a directory still answers for what only it has. */
    for (n, body) in [("vanilla", "the base game's"), ("patch", "the patch's"), ("replacer", "the replacer's")] {
        std::fs::create_dir_all(d.path().join(n).join("meshes")).unwrap();
        std::fs::write(d.path().join(n).join("meshes/shared.nif"), body).unwrap();
    }
    std::fs::write(d.path().join("vanilla/meshes/only_vanilla.nif"), "base only").unwrap();
    let dirs: Vec<(String, PathBuf)> =
        l.roots.iter().enumerate().map(|(i, p)| (format!("r{i}"), p.clone())).collect();
    let v = gardenfell_install::vfs::Vfs::with_roots(&dirs);
    assert_eq!(v.read("meshes/shared.nif").as_deref(), Some(&b"the replacer's"[..]));
    assert_eq!(v.read("meshes/only_vanilla.nif").as_deref(), Some(&b"base only"[..]));
}

// ---------------------------------------------------------------------------------------
// Round 18hk — which Morrowind.ini a plain install reads
// ---------------------------------------------------------------------------------------
//
// abot's report: "apparently cannot read my loading list from my Morrowind.ini", on a GOG
// English install with Data Files picked properly. His ini was fine; the one the program
// read was not his. A `Morrowind.ini` inside Data Files used to be tried before the game
// folder's own and won, and a stray one there (no [Game Files]) made the connect say his
// ini "lists no plugins". Robin: "The Morrowind.ini file to use for Data File vanilla
// installs is the one that sits in the Morrowind folder (so the parent of Data Files)."

const STRAY: &str = "[Weather]\nSunrise Time=6\n";

#[test]
fn the_game_folders_ini_wins_over_a_stray_one_inside_data_files() {
    let d = Dir::new("stray");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    std::fs::write(d.path().join("Morrowind.ini"), INI).unwrap();
    std::fs::write(df.join("Morrowind.ini"), STRAY).unwrap();
    let l = layout::vanilla(&df).expect("the folder's ini");
    assert_eq!(l.order_file, d.path().join("Morrowind.ini"));
    assert!(std::fs::read_to_string(&l.order_file).unwrap().contains("GameFile0=Morrowind.esm"));
    let said = l.warnings.iter().any(|w| w.contains("passed over") && w.contains("Data Files"));
    assert!(said, "the connect log names the ini it passed over: {:?}", l.warnings);
}

#[test]
fn the_game_folders_ini_wins_even_when_both_list_plugins() {
    let d = Dir::new("both");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    std::fs::write(d.path().join("Morrowind.ini"), INI).unwrap();
    std::fs::write(df.join("Morrowind.ini"), "[Game Files]\nGameFile0=Other.esm\n").unwrap();
    let l = layout::vanilla(&df).expect("the folder's ini");
    assert_eq!(l.order_file, d.path().join("Morrowind.ini"));
    assert!(std::fs::read_to_string(&l.order_file).unwrap().contains("GameFile0=Morrowind.esm"));
}

#[test]
fn an_ini_inside_data_files_alone_is_still_read_and_nothing_is_said() {
    let d = Dir::new("inside");
    let df = d.path().join("Data Files");
    std::fs::create_dir_all(&df).unwrap();
    std::fs::write(df.join("Morrowind.ini"), INI).unwrap();
    let l = layout::vanilla(&df).expect("the only ini there is");
    assert_eq!(l.order_file, df.join("Morrowind.ini"));
    assert!(!l.warnings.iter().any(|w| w.contains("passed over")), "{:?}", l.warnings);
}
