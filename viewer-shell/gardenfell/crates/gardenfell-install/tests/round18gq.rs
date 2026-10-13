//! Round 18gq: the install's code page. A Russian edition writes its `Morrowind.ini` in
//! Windows-1251 and its plugins in the same page; until this round `read_to_string` refused
//! the ini ("no plugin load order found") and every record byte was read as Windows-1252.
//! The layout now reads the ini whatever page it is in, says which page that was, and the
//! connect makes it the page in force. An OpenMW install says it in `encoding=`.
//!
//! The two inis a Russian tester sent (one from the disc, one from his own install) are
//! Bethesda's and 1C's text, so they are not in the repository; they sit under
//! `fixture/ru/`, which is ignored, and the tests that read them skip when they are absent.
//! Everything else here is built from bytes.

use gardenfell_bytes::codepage::{self, CodePage};
use gardenfell_install::layout;
use std::path::{Path, PathBuf};

/// The `GameFile` lines of an ini, as text. The engine's own reader is `world::read_load_order`,
/// which sits above this crate; `tests/round18gq.rs` in the world crate runs that over the
/// same files. This only proves the ini came through as letters.
fn game_files(ini: &Path) -> Vec<String> {
    let text = gardenfell_bytes::openmwcfg::order_text(ini);
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("GameFile"))
        .filter_map(|r| r.split_once('=').map(|(_, v)| v.trim().to_string()))
        .collect()
}

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("gf_18gq_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn dir(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A Morrowind.ini the way the Russian game writes one: ASCII keys, Cyrillic question text,
/// and a plugin whose name is Cyrillic too.
fn russian_ini() -> Vec<u8> {
    CodePage::Win1251
        .encode(
            "[General]\r\nSObjectPrefix=\r\n\
             [Question 0]\r\nQuestion=Вы встречаете старого друга в таверне. Что вы делаете?\r\n\
             AnswerOne=Угощаете его выпивкой\r\nAnswerTwo=Расспрашиваете о новостях\r\n\
             [Level Up]\r\nDefault=Вы стали сильнее.\r\n\
             [Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=Tribunal.esm\r\nGameFile2=Трава Вварденфелла.esp\r\n",
        )
        .unwrap()
}

fn western_ini() -> Vec<u8> {
    CodePage::Win1252
        .encode(
            "[Question 0]\r\nQuestion=Ihr trefft einen alten Freund, der über die Straße läuft.\r\n\
             [Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=Tribunal.esm\r\n",
        )
        .unwrap()
}

fn line_about(l: &layout::Layout, what: &str) -> Option<String> {
    l.warnings.iter().find(|w| w.contains(what)).cloned()
}

#[test]
fn a_plain_russian_install_connects_and_names_its_page() {
    let d = Dir::new("vanilla");
    let data = d.dir("Data Files");
    d.dir("Data Files/Meshes");
    d.file("Morrowind.ini", &russian_ini());
    let l = layout::vanilla(&data).expect("the ini is read whatever page it is in");
    assert_eq!(l.code_page, CodePage::Win1251);
    let line = line_about(&l, "Windows-1251").expect("the connect log says which page");
    assert!(line.contains("Morrowind.ini"), "{line}");
    let order = game_files(&l.order_file);
    assert_eq!(order, ["Morrowind.esm", "Tribunal.esm", "Трава Вварденфелла.esp"]);
}

#[test]
fn a_western_install_is_windows_1252_and_says_nothing() {
    let d = Dir::new("western");
    let data = d.dir("Data Files");
    d.dir("Data Files/Meshes");
    d.file("Morrowind.ini", &western_ini());
    let l = layout::vanilla(&data).unwrap();
    assert_eq!(l.code_page, CodePage::Win1252);
    assert!(line_about(&l, "code page").is_none(), "{:?}", l.warnings);
    assert_eq!(game_files(&l.order_file), ["Morrowind.esm", "Tribunal.esm"]);
}

#[test]
fn an_mo2_profile_over_a_russian_game_reads_its_ini() {
    let d = Dir::new("mo2");
    let game = d.dir("game/Data Files");
    d.dir("mo2/mods/Grass");
    d.file("mo2/profiles/Default/modlist.txt", b"+Grass\n");
    d.file("mo2/profiles/Default/Morrowind.ini", &russian_ini());
    d.file(
        "mo2/ModOrganizer.ini",
        format!("[General]\nselected_profile=Default\ngamePath={}\n", game.parent().unwrap().display())
            .as_bytes(),
    );
    let l = layout::mo2(&d.0.join("mo2")).expect("an MO2 layout over a Russian ini");
    assert_eq!(l.code_page, CodePage::Win1251);
    assert!(line_about(&l, "Windows-1251").is_some(), "{:?}", l.warnings);
    assert_eq!(game_files(&l.order_file).len(), 3);
}

#[test]
fn an_openmw_install_says_its_page_with_encoding() {
    let d = Dir::new("openmw");
    d.dir("data");
    let cfg = d.file("openmw.cfg", b"data=\"data\"\ncontent=Morrowind.esm\nencoding=win1251\n");
    let l = layout::openmw(&cfg).unwrap();
    assert_eq!(l.code_page, CodePage::Win1251);
    let line = line_about(&l, "Windows-1251").expect("the connect log says which page");
    assert!(line.contains("encoding=win1251"), "{line}");

    let d2 = Dir::new("openmw-plain");
    d2.dir("data");
    let cfg2 = d2.file("openmw.cfg", b"data=\"data\"\ncontent=Morrowind.esm\n");
    let l2 = layout::openmw(&cfg2).unwrap();
    assert_eq!(l2.code_page, CodePage::Win1252);
    assert!(line_about(&l2, "code page").is_none());

    // A page the engine does not know is the default, not an error: OpenMW itself would
    // refuse the value, so a cfg that carries one does not start the game either.
    let d3 = Dir::new("openmw-odd");
    d3.dir("data");
    let cfg3 = d3.file("openmw.cfg", b"data=\"data\"\ncontent=Morrowind.esm\nencoding=win1253\n");
    assert_eq!(layout::openmw(&cfg3).unwrap().code_page, CodePage::Win1252);
}

#[test]
fn the_overlay_folds_a_cyrillic_path_the_same_way_as_the_disk() {
    use gardenfell_bytes::paths::norm;
    assert_eq!(norm("Meshes\\Трава\\ТРАВА_01.nif"), "meshes/трава/трава_01.nif");
    assert_eq!(norm("Meshes\\Skały\\GŁAZ.nif"), "meshes/skały/głaz.nif");
    assert_eq!(norm("Meshes\\Grass\\G.NIF"), "meshes/grass/g.nif");
    // And a file on disk under the plugin's spelling is found through the overlay.
    let d = Dir::new("overlay");
    d.file("Data Files/Meshes/Трава/Трава.nif", b"NetImmerse File Format, Version 4.0.0.2\n");
    let vfs = gardenfell_install::vfs::Vfs::with_roots(&[("Data Files".to_string(), d.0.join("Data Files"))]);
    let model = CodePage::Win1251.decode(&CodePage::Win1251.encode("ТРАВА\\трава.nif").unwrap());
    assert!(vfs.resolve(&format!("meshes\\{model}")).is_some(), "the Cyrillic mesh resolves");
}

fn fixture_ru() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixture/ru");
    p.is_dir().then_some(p)
}

/// The two inis the tester sent: the disc's and his own install's.
#[test]
fn the_testers_russian_inis_are_windows_1251_and_list_their_plugins() {
    let Some(ru) = fixture_ru() else {
        eprintln!("fixture/ru absent, skipping");
        return;
    };
    let vanilla = ru.join("Morrowind-vanilla.ini");
    let tester = ru.join("Morrowind-tester.ini");
    if !vanilla.is_file() || !tester.is_file() {
        eprintln!("fixture/ru incomplete, skipping");
        return;
    }
    assert_eq!(codepage::sniff_file(&vanilla), CodePage::Win1251);
    assert_eq!(codepage::sniff_file(&tester), CodePage::Win1251);
    let text = codepage::read_text(&vanilla).unwrap();
    assert!(text.contains("[Game Files]"));
    // The disc's questions are Cyrillic and come back as letters, not as Áàëìîðà.
    assert!(text.contains("[Question 0]") || text.contains("[Question 1]"), "the sections are there");
    assert!(text.chars().any(|c| ('а'..='я').contains(&c)), "Cyrillic came through as Cyrillic");
    assert!(!text.contains('Á') && !text.contains('à'), "no Windows-1252 misreading");
    assert_eq!(game_files(&vanilla), ["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"]);
    let order = game_files(&tester);
    assert_eq!(order.len(), 56, "{order:?}");
    assert!(order.iter().any(|n| n == "MFR_RuVoice_v1.0.esm"), "{order:?}");

    // And as a plain install: the ini beside Data Files, read through the layout.
    let d = Dir::new("fixture");
    let data = d.dir("Data Files");
    d.dir("Data Files/Meshes");
    std::fs::copy(&vanilla, d.0.join("Morrowind.ini")).unwrap();
    let l = layout::vanilla(&data).unwrap();
    assert_eq!(l.code_page, CodePage::Win1251);
    assert_eq!(game_files(&l.order_file).len(), 3);
}

/// Round 18gu: the game folder picked for Connect Data Files. The Russian tester's second
/// report: the load order read, every plugin "not found", on his edition and on the English
/// Steam one alike. A game folder has `Morrowind.ini` beside its `Data Files`, so the order
/// read fine and the plugins sat one folder down where no lookup by name reached them. The
/// layout steps into `Data Files` when the picked folder holds no plugin and has one, and
/// says so; a Data Files folder picked properly is left exactly as it was.
#[test]
fn a_game_folder_picked_for_data_files_steps_into_its_data_files() {
    let d = Dir::new("gamefolder");
    let data = d.dir("Morrowind/Data Files");
    d.dir("Morrowind/Data Files/Meshes");
    d.file("Morrowind/Data Files/Morrowind.esm", b"TES3");
    d.file("Morrowind/Morrowind.ini", &western_ini());
    let l = layout::vanilla(&d.0.join("Morrowind")).expect("the game folder connects");
    assert_eq!(l.roots, vec![data.clone()], "the overlay root is the Data Files, not the game folder");
    assert!(l.order_file.ends_with("Morrowind.ini"));
    assert!(line_about(&l, "Data Files folder is used instead").is_some(), "{:?}", l.warnings);
    assert!(line_about(&l, "Meshes").is_none(), "no complaint about meshes: {:?}", l.warnings);
    // Picked properly, nothing changes and nothing is said.
    let l2 = layout::vanilla(&data).unwrap();
    assert_eq!(l2.roots, vec![data.clone()]);
    assert!(line_about(&l2, "used instead").is_none(), "{:?}", l2.warnings);
    // A folder with neither a plugin nor a Data Files under it is taken as it is, as before.
    let d3 = Dir::new("bare");
    let bare = d3.dir("Somewhere");
    d3.file("Somewhere/Morrowind.ini", &western_ini());
    let l3 = layout::vanilla(&bare).unwrap();
    assert_eq!(l3.roots, vec![bare]);
}
