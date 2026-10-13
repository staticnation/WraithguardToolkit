//! Round 18gn, backend-plan part 2 item 15: an `openmw.cfg` is read as OpenMW reads it, as
//! the chain of files its `config=` lines lead to, not as one file.
//!
//! The shapes here are OpenMW's own (`configurationmanager.cpp`): the install's file names
//! the user's folder with `config=`, the lists concatenate in reading order, `replace=`
//! discards what came before, `data-local=` is appended after every `data=` whatever line
//! it stood on, `fallback=` is a map the later file wins, a folder is read once, and a
//! `?token?` at the front of a path names a folder OpenMW knows. The token folders are
//! handed in (`Places`) so the tests say the same thing on every machine.

use gardenfell_bytes::openmwcfg::{read_with, Chain, Places};
use gardenfell_install::layout;
use std::path::{Path, PathBuf};

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("gf_18gn_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn dir(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn cfg(&self, dir: &str, text: &str) -> PathBuf {
        let p = self.dir(dir).join("openmw.cfg");
        std::fs::write(&p, text).unwrap();
        p
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An install folder (`install/openmw.cfg`, the shipped one) whose `config=` leads to the
/// user's folder (`user/openmw.cfg`, the launcher's), with the data folders made so the
/// layout keeps them.
fn install_and_user(d: &Dir) -> (PathBuf, PathBuf, Places) {
    d.dir("install/data");
    d.dir("user/data");
    d.dir("mods/a");
    d.dir("mods/b");
    let root = d.cfg(
        "install",
        "# the install's\n\
         data=\"?local?data\"\n\
         data-local=\"?userdata?data\"\n\
         config=\"?userconfig?\"\n\
         fallback-archive=Morrowind.bsa\n\
         fallback=Weather_Clear_Sky_Day_Color,095,135,203\n\
         fallback=LightAttenuation_UseConstant,1\n\
         content=builtin.omwscripts\n",
    );
    let user = d.cfg(
        "user",
        &format!(
            "data=\"{}\"\n\
             data=\"{}\"\n\
             content=Morrowind.esm\n\
             content=Grass.esp\n\
             groundcover=Grass Vvardenfell.esp\n\
             fallback-archive=Tribunal.bsa\n\
             fallback=Weather_Clear_Sky_Day_Color,100,100,100\n",
            d.path().join("mods/a").display(),
            d.path().join("mods/b").display()
        ),
    );
    let places = Places {
        user_config: Some(d.path().join("user")),
        user_data: Some(d.path().join("user")),
        global_data: None,
        global_config: Some(d.path().join("install")),
    };
    (root, user, places)
}

fn names(c: &Chain) -> Vec<String> {
    c.files.iter().map(|f| f.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned()).collect()
}

#[test]
fn the_install_file_leads_to_the_users_and_the_lists_concatenate_in_that_order() {
    let d = Dir::new("chain");
    let (root, _user, places) = install_and_user(&d);
    let c = read_with(&root, &places);
    assert_eq!(names(&c), ["install", "user"]);
    // data= in reading order, data-local last whatever line it stood on.
    assert_eq!(
        c.data,
        [
            d.path().join("install").join("data"),
            d.path().join("mods/a"),
            d.path().join("mods/b"),
            d.path().join("user").join("data"),
        ]
    );
    assert_eq!(c.content, ["builtin.omwscripts", "Morrowind.esm", "Grass.esp"]);
    assert_eq!(c.groundcover, ["Grass Vvardenfell.esp"]);
    assert_eq!(c.archives, ["Morrowind.bsa", "Tribunal.bsa"]);
    // The map: the user's file wins the key both name, the install's keeps the one only it has.
    let fb = |k: &str| c.fallback.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.as_str());
    assert_eq!(fb("Weather_Clear_Sky_Day_Color"), Some("100,100,100"));
    assert_eq!(fb("LightAttenuation_UseConstant"), Some("1"));
    assert!(c.replaced.is_empty());
}

#[test]
fn pointing_at_the_users_file_reads_the_same_chain_when_the_install_leads_there() {
    let d = Dir::new("fromuser");
    let (root, user, places) = install_and_user(&d);
    let from_root = read_with(&root, &places);
    let from_user = read_with(&user, &places);
    assert_eq!(from_user.files, from_root.files);
    assert_eq!(from_user.text(), from_root.text());
}

#[test]
fn a_file_the_installs_chain_never_reaches_is_read_alone() {
    let d = Dir::new("stranger");
    let (_root, _user, places) = install_and_user(&d);
    d.dir("elsewhere/data");
    let other = d.cfg("elsewhere", "data=data\ncontent=Other.esm\n");
    let c = read_with(&other, &places);
    assert_eq!(names(&c), ["elsewhere"]);
    assert_eq!(c.content, ["Other.esm"]);
    assert_eq!(c.data, [d.path().join("elsewhere").join("data")]);
    assert!(c.archives.is_empty() && c.fallback.is_empty());
}

#[test]
fn replace_discards_what_earlier_files_said_for_that_option_only() {
    let d = Dir::new("replace");
    let (root, _user, places) = install_and_user(&d);
    std::fs::write(
        d.path().join("user/openmw.cfg"),
        "replace=content\nreplace=fallback-archive\ncontent=Morrowind.esm\nfallback-archive=Only.bsa\n",
    )
    .unwrap();
    let c = read_with(&root, &places);
    assert_eq!(c.content, ["Morrowind.esm"], "builtin.omwscripts from the install is gone");
    assert_eq!(c.archives, ["Only.bsa"]);
    assert_eq!(c.data.len(), 2, "data= was not replaced: the install's and data-local");
    assert_eq!(c.fallback.len(), 2, "fallback= was not replaced");
    let mut r = c.replaced.clone();
    r.sort();
    assert_eq!(r, ["content", "fallback-archive"]);
}

#[test]
fn replace_config_keeps_only_the_root_before_it() {
    let d = Dir::new("replcfg");
    d.dir("root");
    d.dir("one");
    d.dir("two");
    let root = d.cfg(
        "root",
        &format!(
            "config={}\nconfig={}\ncontent=Root.esm\n",
            d.path().join("one").display(),
            d.path().join("two").display()
        ),
    );
    d.cfg("one", "content=One.esm\n");
    d.cfg("two", "replace=config\ncontent=Two.esm\n");
    let c = read_with(&root, &Places::default());
    // `one` was read, then `two` said replace=config and threw it away.
    assert_eq!(names(&c), ["root", "two"]);
    assert_eq!(c.content, ["Root.esm", "Two.esm"]);
}

#[test]
fn a_folder_is_read_once_and_a_cycle_ends() {
    let d = Dir::new("cycle");
    let a = d.cfg(
        "a",
        &format!(
            "config={}\nconfig={}\ncontent=A.esm\n",
            d.path().join("b").display(),
            d.path().join("b").display()
        ),
    );
    d.cfg("b", &format!("config={}\ncontent=B.esm\n", d.path().join("a").display()));
    let c = read_with(&a, &Places::default());
    assert_eq!(names(&c), ["a", "b"]);
    assert_eq!(c.content, ["A.esm", "B.esm"]);
}

#[test]
fn config_folders_are_read_in_the_order_listed_depth_first() {
    let d = Dir::new("order");
    let root = d.cfg(
        "root",
        &format!("config={}\nconfig={}\n", d.path().join("one").display(), d.path().join("three").display()),
    );
    d.cfg("one", &format!("config={}\ncontent=One.esm\n", d.path().join("two").display()));
    d.cfg("two", "content=Two.esm\n");
    d.cfg("three", "content=Three.esm\n");
    let c = read_with(&root, &Places::default());
    // OpenMW pushes a file's config= folders onto a stack first-listed on top, so `one` is
    // read, then what `one` named, then `three`.
    assert_eq!(names(&c), ["root", "one", "two", "three"]);
    assert_eq!(c.content, ["One.esm", "Two.esm", "Three.esm"]);
}

#[test]
fn tokens_name_folders_relative_paths_name_their_own_files_folder_and_an_unknown_token_names_nothing() {
    let d = Dir::new("tokens");
    let root = d.cfg(
        "inst",
        "data=\"?local?data\"\ndata=\"?global?share\"\ndata=\"?nosuch?x\"\ndata=rel/ative\ndata-local=\"?userdata?data\"\n",
    );
    let places = Places {
        user_config: None,
        user_data: Some(PathBuf::from("/tmp/ud")),
        global_data: Some(PathBuf::from("/usr/share/games/openmw")),
        global_config: None,
    };
    let c = read_with(&root, &places);
    let inst = d.path().join("inst");
    assert_eq!(
        c.data,
        [
            inst.join("data"),
            PathBuf::from("/usr/share/games/openmw").join("share"),
            PathBuf::new(),
            inst.join("rel/ative"),
            PathBuf::from("/tmp/ud").join("data"),
        ]
    );
}

#[test]
fn quoted_paths_lose_their_quotes_and_openmws_ampersand_escapes() {
    let d = Dir::new("quotes");
    // Relative to the file's folder, like any unrooted path; the point is the unquoting.
    let root = d.cfg("q", "data=\"Rock && Roll/&\"Data Files&\"\"\n");
    let c = read_with(&root, &Places::default());
    assert_eq!(c.data, [d.path().join("q").join("Rock & Roll").join("\"Data Files\"")]);
}

#[test]
fn the_layout_takes_the_chains_folders_last_listed_first_and_names_the_files_read() {
    let d = Dir::new("layout");
    let (_root, user, places) = install_and_user(&d);
    // `layout::openmw` reads with the machine's own places, which do not lead to this
    // temporary folder, so it is handed the chain's text through the user file alone here
    // and the chain itself through `read_with`; both say the same about the folders.
    let c = read_with(&user, &places);
    let mut want: Vec<PathBuf> = c.data.clone();
    want.reverse();
    assert_eq!(want[0], d.path().join("user").join("data"), "data-local is the top of the overlay");
    let l = layout::openmw(&user).expect("reads");
    // On this machine the install is not found from here, so the one file is read: its
    // two folders, nothing else, and no chain line in the log.
    assert_eq!(l.roots, [d.path().join("mods/b"), d.path().join("mods/a")]);
    assert!(!l.warnings.iter().any(|w| w.contains("chain")), "{:?}", l.warnings);
}

#[test]
fn the_merged_text_is_what_the_readers_of_an_order_file_see() {
    let d = Dir::new("text");
    let (root, _user, places) = install_and_user(&d);
    let text = read_with(&root, &places).text();
    // Every reader of an order file's text sees one `fallback=` per key with the winning
    // value, the content in load order, the archives in order.
    assert!(text.contains("fallback=Weather_Clear_Sky_Day_Color,100,100,100\n"));
    assert!(!text.contains("095,135,203"));
    assert!(text.contains("content=builtin.omwscripts\ncontent=Morrowind.esm\ncontent=Grass.esp\n"));
    assert!(text.contains("fallback-archive=Morrowind.bsa\nfallback-archive=Tribunal.bsa\n"));
    assert!(text.contains("groundcover=Grass Vvardenfell.esp\n"));
    assert!(text.starts_with("# openmw.cfg as OpenMW reads it, merged from:\n"));
    // The last folder read is the user's, where settings.cfg lives.
    assert_eq!(read_with(&root, &places).user_config_dir(), Some(d.path().join("user")));
}
