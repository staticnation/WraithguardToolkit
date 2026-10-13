//! Which MGE is installed, and therefore where the fog settings live — round 17m.
//!
//! Robin: "What are the values for fog from MGE that you find on my Morrowind install?"
//! The tool answered from `mge3\MGE.ini`, which his game had not read in three months: he
//! runs G7's fork of MGE XE, which keeps everything in `mgeXE.toml`. The preview was
//! fogging at 5 cells where the game fogs at 8 — nearly a factor of two in how far you
//! can see, and the cause of a run of "the fog looks wrong" reports before it.
//!
//! The question is put to the shim Morrowind loads. `d3d8.dll` *is* MGE, and each build
//! names its own config file inside itself. Checked by hand against all three builds that
//! exist, counting case-insensitive occurrences:
//!
//! | build                 | `mgeXE.toml` | `MGE.ini` |
//! |-----------------------|--------------|-----------|
//! | MGE-XE G7 Fork v0.20.0 | 2           | 0         |
//! | MGE XE (Hrnchamd)      | 0           | 1         |
//! | MGE XE UF 0.19.1       | 0           | 1         |
//!
//! Both ini builds spell it `MGE3/MGE.ini`. The fixtures below carry those exact marker
//! strings rather than real DLLs, because what is under test is the question, not the PE
//! format.
//!
//! Robin also asked whether to decide by file date instead. The fork's README rules it
//! out — "We do not migrate `MGE.ini` into `mgeXE.toml`" — so the two files are
//! independent and a date says only when that file's own GUI last saved. It survives here
//! as the last resort, for an install with no MGE at all.

use gardenfell_install::layout::Layout;
use gardenfell_install::mge::{self, ConfigKind};
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("gdn-mge-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A stand-in shim carrying the marker strings the real build carries, padded so the
/// scan has something to walk past.
fn dll(path: &Path, markers: &[&str]) {
    let mut b = vec![0u8; 4096];
    b.extend_from_slice(b"MZ\x90\x00 this is not really a PE file ");
    for m in markers {
        b.extend_from_slice(m.as_bytes());
        b.extend_from_slice(&[0u8; 64]);
    }
    b.extend_from_slice(&vec![0u8; 4096]);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b).unwrap();
}

const G7: &[&str] = &["mgeXE.toml", "mgeHost64", "MGE-XE G7 Fork v0.20.0"];
// UF names mgeHost64 too — which is why the fork is not identified by that.
const UF: &[&str] = &["MGE3/MGE.ini", "mgeXE.log", "mgeHost64", "MGE XE 0.19.1"];
const STOCK: &[&str] = &["MGE3/MGE.ini", "MGE XE"];

const TOML: &str = r#"
schema_version = 3
[distant_land.fog]
exponential = true
atmosphere_scattering = true
above_water_start = 2.32
above_water_end = 8.0
below_water_start = -0.2
below_water_end = 0.8
[distant_land.weather.clear]
wind = 0.1
fog_ratio = 1.0
fog_offset = 0.0
[generation]
data_dirs = [
    'C:\Games\Morrowind\Data Files',
    '''C:\Games\Morrowind\Data Files\Robin's grass''',
]
"#;

const INI: &str = "[Distant Land]\r\nAbove Water Fog Start=2\r\nAbove Water Fog End=5\r\n\
                   Use Exponential Fog=True\r\nUse Atmosphere Scattering=True\r\n\
                   [Distant Land Weather]\r\nClear Fog Ratio=1.00\r\nClear Fog Offset=0.00\r\n";

/// A game folder holding both config files and, optionally, a shim.
fn game(dir: &Path, shim: Option<&[&str]>) {
    std::fs::create_dir_all(dir.join("mge3")).unwrap();
    std::fs::write(dir.join("mgeXE.toml"), TOML).unwrap();
    std::fs::write(dir.join("mge3").join("MGE.ini"), INI).unwrap();
    if let Some(m) = shim {
        dll(&dir.join("d3d8.dll"), m);
    }
}

fn plain(dir: &Path) -> Layout {
    Layout { game_dir: Some(dir.to_path_buf()), ..Default::default() }
}

// ---- the shim answers the question ------------------------------------------------

#[test]
fn each_build_names_the_config_file_it_reads() {
    assert_eq!(mge::dll_kind(b"...mgeXE.toml...mgeHost64..."), Some(ConfigKind::Toml));
    assert_eq!(mge::dll_kind(b"...~dkMGE3/MGE.ini..."), Some(ConfigKind::Ini));
    // Case does not matter: it is a binary, not a document.
    assert_eq!(mge::dll_kind(b"MGEXE.TOML"), Some(ConfigKind::Toml));
    // Something that is not MGE at all says nothing, and must not be guessed at.
    assert_eq!(mge::dll_kind(b"just some other dll entirely"), None);
    /* A build that read both would be answered with the newer format; the reasoning is
       shown to the reader either way. */
    assert_eq!(mge::dll_kind(b"mgeXE.toml and MGE.ini"), Some(ConfigKind::Toml));
}

#[test]
fn the_fork_is_read_from_its_toml_and_the_others_from_the_ini() {
    for (name, markers, want, file) in [
        ("g7", G7, ConfigKind::Toml, "mgeXE.toml"),
        ("uf", UF, ConfigKind::Ini, "mge3\\MGE.ini"),
        ("stock", STOCK, ConfigKind::Ini, "mge3\\MGE.ini"),
    ] {
        let d = tmp(name);
        game(&d, Some(markers));
        let f = mge::find(&plain(&d));
        assert_eq!(f.kind, Some(want), "{name}: {}", f.how);
        assert_eq!(f.kind.unwrap().file(), file);
        assert!(f.config.as_ref().unwrap().exists(), "{name}");
        assert!(f.how.contains("game folder"), "{name}: {}", f.how);
    }
}

/// Robin's worry, and the reason the date is not the test: "a user installs G7's MGE XE
/// fork, and then for some reason like compatibility, install another version, which
/// would let mgeXE.toml linger even when it's no longer used in the game."
#[test]
fn a_lingering_toml_beside_a_stock_shim_is_ignored() {
    let d = tmp("lingering");
    game(&d, Some(STOCK));
    // And make the stale toml the *newer* file, which is the trap.
    let now = std::time::SystemTime::now();
    filetime_touch(&d.join("mgeXE.toml"), now);
    let f = mge::find(&plain(&d));
    assert_eq!(f.kind, Some(ConfigKind::Ini), "{}", f.how);
}

fn filetime_touch(p: &Path, _t: std::time::SystemTime) {
    // Rewriting is enough to make it the newest file on any platform.
    let text = std::fs::read(p).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(p, text).unwrap();
}

#[test]
fn with_no_mge_at_all_the_newer_config_is_taken_and_said_so() {
    let d = tmp("nomge");
    game(&d, None);
    filetime_touch(&d.join("mgeXE.toml"), std::time::SystemTime::now());
    let f = mge::find(&plain(&d));
    assert_eq!(f.kind, Some(ConfigKind::Toml), "{}", f.how);
    assert!(f.how.contains("no MGE found"), "{}", f.how);
    // Only one present: no guessing needed.
    std::fs::remove_file(d.join("mgeXE.toml")).unwrap();
    let f = mge::find(&plain(&d));
    assert_eq!(f.kind, Some(ConfigKind::Ini));
    assert!(f.how.contains("only MGE.ini"), "{}", f.how);
}

#[test]
fn nothing_at_all_is_reported_rather_than_defaulted_silently() {
    let d = tmp("empty");
    std::fs::create_dir_all(&d).unwrap();
    let f = mge::find(&plain(&d));
    assert_eq!(f.kind, None);
    assert!(f.config.is_none());
    assert_eq!(f.how, "no MGE settings found");
}

// ---- Root Builder ------------------------------------------------------------------

/// An MO2 layout: `roots` in the order the overlay resolves them — overwrite first, then
/// the mods top-down — which is the order Root Builder resolves its own files in too.
fn mo2(game_dir: &Path, base: &Path, roots: &[PathBuf]) -> Layout {
    Layout {
        game_dir: Some(game_dir.to_path_buf()),
        mo2_base: Some(base.to_path_buf()),
        roots: roots.to_vec(),
        ..Default::default()
    }
}

/// With the game shut, Root Builder has *restored the original* into the game folder —
/// so the shim lying there is the one that will not load. Reading it is not merely
/// incomplete for these users, it is backwards.
#[test]
fn the_shim_in_a_root_mod_beats_the_one_in_the_game_folder() {
    let d = tmp("rb-mod");
    let (g, m) = (d.join("game"), d.join("mo2"));
    game(&g, Some(STOCK));                                   // the decoy
    dll(&m.join("mods/MGE G7/Root/d3d8.dll"), G7);           // what actually loads
    let f = mge::find(&mo2(&g, &m, &[m.join("mods/MGE G7")]));
    assert_eq!(f.kind, Some(ConfigKind::Toml), "{}", f.how);
    assert!(f.how.contains("Root Builder mod"), "{}", f.how);
}

#[test]
fn root_mods_resolve_in_mo2_priority_with_overwrite_on_top() {
    let d = tmp("rb-order");
    let (g, m) = (d.join("game"), d.join("mo2"));
    game(&g, Some(STOCK));
    dll(&m.join("mods/High/Root/d3d8.dll"), G7);
    dll(&m.join("mods/Low/Root/d3d8.dll"), UF);
    // Highest priority first, as `modlist.txt` is written and as `roots` is built.
    let hi_first = [m.join("mods/High"), m.join("mods/Low")];
    assert_eq!(mge::find(&mo2(&g, &m, &hi_first)).kind, Some(ConfigKind::Toml));
    let lo_first = [m.join("mods/Low"), m.join("mods/High")];
    assert_eq!(mge::find(&mo2(&g, &m, &lo_first)).kind, Some(ConfigKind::Ini));
    // `overwrite\Root` sits above every mod.
    dll(&m.join("overwrite/Root/d3d8.dll"), UF);
    let with_ow = [m.join("overwrite"), m.join("mods/High"), m.join("mods/Low")];
    assert_eq!(mge::find(&mo2(&g, &m, &with_ow)).kind, Some(ConfigKind::Ini));
}

/// Root Builder's own record of what is deployed *right now* — the file it uses as that
/// same test, and the only thing that catches a build left behind by a crashed session.
#[test]
fn a_live_build_record_outranks_the_mod_order() {
    let d = tmp("rb-build");
    let (g, m) = (d.join("game"), d.join("mo2"));
    game(&g, Some(STOCK));
    dll(&m.join("mods/G7/Root/d3d8.dll"), G7);
    dll(&m.join("overwrite/Root/d3d8.dll"), UF);   // would win on priority alone
    let src = m.join("mods/G7/Root/d3d8.dll");
    let data = m.join("plugins/data/RootBuilder/CGamesMorrowind/1_6_1820");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("BuildData.json"),
        format!(
            r#"{{"Copy":{{"mwse.dll":{{"Source":"/nowhere/MWSE.dll","Relative":"MWSE.dll"}},
                "d3d8.dll":{{"Source":{:?},"Relative":"d3d8.dll","Hash":"ab"}}}},
               "Link":{{}},"USVFS":{{}}}}"#,
            src.display().to_string()
        ),
    )
    .unwrap();
    let roots = [m.join("overwrite"), m.join("mods/G7")];
    let f = mge::find(&mo2(&g, &m, &roots));
    assert_eq!(f.kind, Some(ConfigKind::Toml), "{}", f.how);
    assert_eq!(f.dll.as_deref(), Some(src.as_path()));
    assert!(f.how.contains("in Root Builder"), "{}", f.how);
}

#[test]
fn the_build_record_is_read_without_a_json_parser() {
    // Keys are the relative path lowercased; `Source` is absolute and backslash-escaped.
    let j = r#"{"Copy":{"d3d8.dll":{"Source":"C:\\MO2\\mods\\MGE XE\\Root\\d3d8.dll",
                "Relative":"d3d8.dll","Hash":"x"}},"Link":{},"USVFS":{}}"#;
    assert_eq!(
        mge::build_data_source(j, "d3d8.dll").as_deref(),
        Some("C:\\MO2\\mods\\MGE XE\\Root\\d3d8.dll")
    );
    // A build that deploys other things but not this one answers nothing.
    let none = r#"{"Copy":{"mwse.dll":{"Source":"C:\\x\\MWSE.dll"}},"Link":{},"USVFS":{}}"#;
    assert_eq!(mge::build_data_source(none, "d3d8.dll"), None);
    // Link and USVFS entries count too — the file is deployed either way.
    let link = r#"{"Copy":{},"Link":{"d3d8.dll":{"Source":"/m/Root/d3d8.dll"}},"USVFS":{}}"#;
    assert_eq!(mge::build_data_source(link, "d3d8.dll").as_deref(), Some("/m/Root/d3d8.dll"));
}

/// A `Source` naming a file that is not there — a stale record, or a Windows path being
/// read somewhere else — is ignored rather than believed.
#[test]
fn a_build_record_pointing_at_nothing_falls_through() {
    let d = tmp("rb-stale");
    let (g, m) = (d.join("game"), d.join("mo2"));
    game(&g, Some(STOCK));
    dll(&m.join("mods/G7/Root/d3d8.dll"), G7);
    let data = m.join("plugins/data/RootBuilder/X/Y");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("BuildData.json"),
        r#"{"Copy":{"d3d8.dll":{"Source":"C:\\gone\\d3d8.dll"}},"Link":{},"USVFS":{}}"#,
    )
    .unwrap();
    let f = mge::find(&mo2(&g, &m, &[m.join("mods/G7")]));
    assert_eq!(f.kind, Some(ConfigKind::Toml), "{}", f.how);
    assert!(f.how.contains("Root Builder mod"), "{}", f.how);
}

/// A mod may ship the config file in its `Root\` as well, and then that is the one the
/// game reads.
#[test]
fn a_config_deployed_by_a_root_mod_is_the_one_read() {
    let d = tmp("rb-config");
    let (g, m) = (d.join("game"), d.join("mo2"));
    game(&g, Some(G7));
    let modded = m.join("mods/Settings/Root/mgeXE.toml");
    std::fs::create_dir_all(modded.parent().unwrap()).unwrap();
    std::fs::write(&modded, TOML.replace("above_water_end = 8.0", "above_water_end = 3.5")).unwrap();
    let f = mge::find(&mo2(&g, &m, &[m.join("mods/Settings")]));
    assert_eq!(f.config.as_deref(), Some(modded.as_path()), "{}", f.how);
}

// ---- and the numbers that come out of it -------------------------------------------

fn fog(l: &Layout) -> String {
    gardenfell_install::weather::mge_json(l)
}

#[test]
fn the_fog_numbers_come_from_whichever_file_won() {
    let d = tmp("numbers");
    let g7 = d.join("g7");
    game(&g7, Some(G7));
    let j = fog(&plain(&g7));
    assert!(j.contains("\"fogStart\":2.32"), "{j}");
    assert!(j.contains("\"fogEnd\":8"), "{j}");
    assert!(j.contains("\"name\":\"mgeXE.toml\""), "{j}");
    assert!(j.contains("\"fogRatio\":1"), "{j}");
    // Round 18dt: and the below-water pair, the fork's own names.
    assert!(j.contains("\"fogBelowStart\":-0.2"), "{j}");
    assert!(j.contains("\"fogBelowEnd\":0.8"), "{j}");

    let stock = d.join("stock");
    game(&stock, Some(STOCK));
    let j = fog(&plain(&stock));
    assert!(j.contains("\"fogStart\":2"), "{j}");
    assert!(j.contains("\"fogEnd\":5"), "{j}");
    assert!(j.contains("mge3\\\\MGE.ini"), "{j}");
}

/// The whole point of the round: the answer must say where it came from, because an
/// invisible wrong answer is what hid the original bug for three months.
#[test]
fn the_answer_names_the_file_and_the_reasoning() {
    let d = tmp("shown");
    game(&d, Some(G7));
    let j = fog(&plain(&d));
    assert!(j.contains("\"how\":\"d3d8.dll in the game folder reads mgeXE.toml\""), "{j}");
    assert!(j.contains("\"file\":\""), "{j}");
    assert!(j.contains("d3d8.dll"), "{j}");
}

/// A config that will not parse must complain, not quietly become MGE's defaults.
#[test]
fn a_broken_config_says_so_instead_of_defaulting_in_silence() {
    let d = tmp("broken");
    game(&d, Some(G7));
    std::fs::write(d.join("mgeXE.toml"), "[distant_land.fog\nabove_water_end = ").unwrap();
    let j = fog(&plain(&d));
    assert!(j.contains("could not be read"), "{j}");
}
