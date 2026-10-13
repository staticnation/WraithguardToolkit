//! Round 18bi — choosing which plugins to load, and the dependency rule underneath it.
//!
//! Robin: "To make it easier for mod creators, I want to add options to choose which of the
//! available .esp in one's install should be loaded." The choosing happens before anything
//! is parsed, because his own order is 562 plugins deep and skipping that wait is the point.
//!
//! The rule he designed, and what these tests hold to it: a tick box has **three** states —
//! empty, ticked, and ticked-because-something-needs-it, drawn as a dash. "Anything that
//! requires a master will automatically tick the master too, but with a smaller priority
//! than ticking itself […] When checked due to being a requirement, it will automatically
//! untick if the entry requiring it is unticked. […] Can't untick a -, and if it's a check
//! mark, it will turn into a -."
//!
//! The page draws that; `plugins::with_masters` is the engine's own copy of it, so that a
//! run that never opened the dialogue cannot ask for a load order the game would refuse.

use gardenfell_tes3::esp;
use gardenfell_install::plugins::{self, Entry};

// ---- a plugin header, built byte by byte ------------------------------------------------

fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut o = tag.to_vec();
    o.extend_from_slice(&(data.len() as u32).to_le_bytes());
    o.extend_from_slice(data);
    o
}

fn hedr(desc: &str) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&1.3f32.to_le_bytes()); // version
    d.extend_from_slice(&0u32.to_le_bytes()); // file type
    d.extend_from_slice(&[0u8; 32]); // company
    let mut text = desc.as_bytes().to_vec();
    text.resize(256, 0);
    d.extend_from_slice(&text);
    d.extend_from_slice(&0u32.to_le_bytes()); // record count
    sub(b"HEDR", &d)
}

fn zstr(s: &str) -> Vec<u8> {
    let mut o = s.as_bytes().to_vec();
    o.push(0);
    o
}

/// A plugin file: the TES3 record, then whatever junk stands in for its contents. The
/// junk matters — it is what proves the reader stops at the header.
fn plugin(desc: &str, masters: &[&str], tail: usize) -> Vec<u8> {
    let mut body = hedr(desc);
    for m in masters {
        body.extend_from_slice(&sub(b"MAST", &zstr(m)));
        body.extend_from_slice(&sub(b"DATA", &0u64.to_le_bytes()));
    }
    let mut o = b"TES3".to_vec();
    o.extend_from_slice(&(body.len() as u32).to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes()); // unknown
    o.extend_from_slice(&0u32.to_le_bytes()); // flags
    o.extend_from_slice(&body);
    o.extend_from_slice(&vec![0xCDu8; tail]);
    o
}

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Dir {
        let mut p = std::env::temp_dir();
        p.push(format!("gf_18bi_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn put(&self, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entry(name: &str, masters: &[&str]) -> Entry {
    Entry {
        name: name.into(),
        masters: masters.iter().map(|m| m.to_ascii_lowercase()).collect(),
        esm: plugins::is_master(name),
        ..Default::default()
    }
}

// ---- reading a header ------------------------------------------------------------------

#[test]
fn a_headers_masters_are_read_in_the_order_it_names_them() {
    let h = esp::plugin_head(&plugin("", &["Morrowind.esm", "Tribunal.esm"], 0)).expect("a plugin");
    assert_eq!(h.masters, vec!["morrowind.esm", "tribunal.esm"]);
}

#[test]
fn master_names_come_back_lower_cased_the_way_the_engine_compares_them() {
    let h = esp::plugin_head(&plugin("", &["MORROWIND.ESM"], 0)).unwrap();
    assert_eq!(h.masters, vec!["morrowind.esm"]);
}

#[test]
fn a_plugin_with_no_masters_reads_fine_and_has_none() {
    let h = esp::plugin_head(&plugin("", &[], 0)).expect("no masters is not a failure");
    assert!(h.masters.is_empty());
}

#[test]
fn the_authors_description_comes_back_trimmed_of_its_padding() {
    let h = esp::plugin_head(&plugin("Grass for the Bitter Coast", &[], 0)).unwrap();
    assert_eq!(h.desc, "Grass for the Bitter Coast");
}

#[test]
fn something_that_is_not_a_plugin_is_refused_rather_than_guessed_at() {
    assert!(esp::plugin_head(b"not a plugin at all").is_none());
    assert!(esp::plugin_head(b"").is_none());
    assert!(esp::plugin_head(b"TES3").is_none(), "a tag alone is not a header");
}

#[test]
fn the_header_is_read_without_reading_the_plugin() {
    /* The whole reason this exists. `head_len` says how much of the file is needed, and
       for a plugin with two masters that is a couple of hundred bytes whether the plugin
       is 2 KB or 200 MB. Robin's order has Tamriel Rebuilt in it. */
    let big = plugin("", &["Morrowind.esm", "Tamriel_Data.esm"], 4 * 1024 * 1024);
    let need = esp::head_len(&big[..16]).expect("a plugin's header states its own length");
    assert!(need < 1024, "the header wanted {need} bytes of a {} byte file", big.len());
    let h = esp::plugin_head(&big[..need]).expect("read from the header alone");
    assert_eq!(h.masters, vec!["morrowind.esm", "tamriel_data.esm"]);
}

#[test]
fn reading_from_disk_touches_only_the_head_of_the_file() {
    let d = Dir::new("disk");
    let p = d.put("Big.esp", &plugin("", &["Morrowind.esm"], 2 * 1024 * 1024));
    let h = plugins::head_of(&p).expect("read off disk");
    assert_eq!(h.masters, vec!["morrowind.esm"]);
}

#[test]
fn a_file_that_is_there_but_is_not_a_plugin_reads_as_nothing() {
    let d = Dir::new("junk");
    let p = d.put("Junk.esp", b"this is not a plugin");
    assert!(plugins::head_of(&p).is_none());
}

#[test]
fn a_header_claiming_an_absurd_length_does_not_get_what_it_asked_for() {
    // A damaged file states a header longer than the file. The reader must not trust it.
    let d = Dir::new("absurd");
    let mut bytes = b"TES3".to_vec();
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 8]);
    let p = d.put("Bad.esp", &bytes);
    let h = plugins::head_of(&p);
    assert!(h.map(|x| x.masters.is_empty()).unwrap_or(true));
}

// ---- the list ---------------------------------------------------------------------------

#[test]
fn the_list_keeps_the_order_file_s_own_order() {
    // Robin: "we want to show the .esm and .esp files in the load order they were loaded in".
    let d = Dir::new("order");
    d.put("Morrowind.esm", &plugin("", &[], 0));
    d.put("B.esp", &plugin("", &["Morrowind.esm"], 0));
    d.put("A.esp", &plugin("", &["Morrowind.esm"], 0));
    let order: Vec<String> = ["Morrowind.esm", "B.esp", "A.esp"].iter().map(|s| s.to_string()).collect();
    let got = plugins::list(&order, &order, |n| Some(d.0.join(n)).filter(|p| p.exists()));
    let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Morrowind.esm", "B.esp", "A.esp"], "not sorted, not regrouped");
}

#[test]
fn a_plugin_the_order_names_but_the_install_lacks_is_listed_as_missing() {
    let d = Dir::new("gone");
    d.put("Morrowind.esm", &plugin("", &[], 0));
    let order: Vec<String> = ["Morrowind.esm", "Ghost.esp"].iter().map(|s| s.to_string()).collect();
    let got = plugins::list(&order, &order, |n| Some(d.0.join(n)).filter(|p| p.exists()));
    assert!(!got[0].missing);
    assert!(got[1].missing, "named by the order, not on disk");
    assert!(!got[1].unreadable, "missing and unreadable are different things");
}

#[test]
fn a_plugin_that_is_there_but_will_not_read_says_so_separately() {
    let d = Dir::new("bad");
    d.put("Broken.esp", b"");
    let order = vec!["Broken.esp".to_string()];
    let got = plugins::list(&order, &order, |n| Some(d.0.join(n)).filter(|p| p.exists()));
    assert!(!got[0].missing, "it is on disk");
    assert!(got[0].unreadable);
}

#[test]
fn masters_are_marked_by_their_extension() {
    assert!(plugins::is_master("Morrowind.esm"));
    assert!(plugins::is_master("TAMRIEL_DATA.ESM"));
    assert!(plugins::is_master("game.omwgame"));
    assert!(!plugins::is_master("MyGrass.esp"));
    assert!(!plugins::is_master("thing.omwaddon"));
}

// ---- narrowing --------------------------------------------------------------------------

#[test]
fn narrowing_to_nothing_named_keeps_the_whole_order() {
    // `None` means nobody chose, which is everything — never nothing, which is the failure
    // mode worth being deliberate about.
    let order: Vec<String> = ["a.esm", "b.esp"].iter().map(|s| s.to_string()).collect();
    assert_eq!(plugins::narrow(&order, None), order);
}

#[test]
fn narrowing_keeps_the_load_order_not_the_order_they_were_named_in() {
    let order: Vec<String> = ["a.esm", "b.esp", "c.esp"].iter().map(|s| s.to_string()).collect();
    let want: Vec<String> = ["c.esp", "a.esm"].iter().map(|s| s.to_string()).collect();
    assert_eq!(plugins::narrow(&order, Some(&want)), vec!["a.esm", "c.esp"]);
}

#[test]
fn narrowing_does_not_care_about_case() {
    let order: Vec<String> = ["Morrowind.esm", "MyGrass.esp"].iter().map(|s| s.to_string()).collect();
    let want = vec!["MORROWIND.ESM".to_string()];
    assert_eq!(plugins::narrow(&order, Some(&want)), vec!["Morrowind.esm"]);
}

#[test]
fn narrowing_to_an_empty_list_really_does_mean_none() {
    let order: Vec<String> = ["a.esm"].iter().map(|s| s.to_string()).collect();
    assert!(plugins::narrow(&order, Some(&[])).is_empty());
}

// ---- the dependency rule ----------------------------------------------------------------

fn world() -> Vec<Entry> {
    vec![
        entry("Morrowind.esm", &[]),
        entry("Tribunal.esm", &["Morrowind.esm"]),
        entry("Tamriel_Data.esm", &["Morrowind.esm"]),
        entry("TR_Mainland.esm", &["Morrowind.esm", "Tamriel_Data.esm"]),
        entry("TR_Preview.esp", &["Morrowind.esm", "Tamriel_Data.esm", "TR_Mainland.esm"]),
        entry("MyGrass.esp", &["Morrowind.esm"]),
    ]
}

#[test]
fn choosing_a_plugin_pulls_in_the_master_it_rests_on() {
    let got = plugins::with_masters(&world(), &["MyGrass.esp".into()]);
    assert_eq!(got, vec!["Morrowind.esm", "MyGrass.esp"]);
}

#[test]
fn the_pull_goes_all_the_way_down_not_one_step() {
    // TR_Preview needs TR_Mainland, which needs Tamriel_Data, which needs Morrowind.
    let got = plugins::with_masters(&world(), &["TR_Preview.esp".into()]);
    assert_eq!(got, vec!["Morrowind.esm", "Tamriel_Data.esm", "TR_Mainland.esm", "TR_Preview.esp"]);
}

#[test]
fn what_comes_back_is_in_load_order_whatever_order_it_was_chosen_in() {
    let got = plugins::with_masters(&world(), &["MyGrass.esp".into(), "Tribunal.esm".into()]);
    assert_eq!(got, vec!["Morrowind.esm", "Tribunal.esm", "MyGrass.esp"]);
}

#[test]
fn dropping_the_thing_that_needed_it_drops_the_master_with_it() {
    /* Robin's rule: "When checked due to being a requirement […] it will automatically
       untick if the entry requiring it is unticked." Nothing chose Tamriel_Data here; it
       was only ever there because TR_Mainland was. */
    let with = plugins::with_masters(&world(), &["TR_Mainland.esm".into()]);
    assert!(with.contains(&"Tamriel_Data.esm".to_string()));
    let without = plugins::with_masters(&world(), &["MyGrass.esp".into()]);
    assert!(!without.contains(&"Tamriel_Data.esm".to_string()));
}

#[test]
fn a_master_chosen_in_its_own_right_stays_when_its_dependent_goes() {
    // The other half of the same rule: an explicit tick outranks being required, so it
    // survives what required it going away.
    let got = plugins::with_masters(&world(), &["Tamriel_Data.esm".into()]);
    assert_eq!(got, vec!["Morrowind.esm", "Tamriel_Data.esm"]);
}

#[test]
fn choosing_nothing_loads_nothing() {
    assert!(plugins::with_masters(&world(), &[]).is_empty());
}

#[test]
fn choosing_everything_is_the_whole_order_unchanged() {
    let all = world();
    let every: Vec<String> = all.iter().map(|e| e.name.clone()).collect();
    assert_eq!(plugins::with_masters(&all, &every), every);
}

#[test]
fn a_master_the_install_does_not_have_cannot_be_pulled_in_by_wanting_it() {
    // An order naming a plugin whose master is not installed: the closure must not invent
    // a row for it. The connect report is what says the master is missing.
    let all = vec![entry("Orphan.esp", &["NotHere.esm"])];
    assert_eq!(plugins::with_masters(&all, &["Orphan.esp".into()]), vec!["Orphan.esp"]);
}

#[test]
fn a_cycle_between_masters_terminates() {
    // Not a thing a sane plugin does, and exactly the thing a damaged one does.
    let all = vec![entry("A.esp", &["B.esp"]), entry("B.esp", &["A.esp"])];
    assert_eq!(plugins::with_masters(&all, &["A.esp".into()]), vec!["A.esp", "B.esp"]);
}

#[test]
fn a_plugin_naming_itself_as_a_master_terminates_too() {
    let all = vec![entry("Self.esp", &["Self.esp"])];
    assert_eq!(plugins::with_masters(&all, &["Self.esp".into()]), vec!["Self.esp"]);
}

#[test]
fn choosing_by_another_spelling_still_finds_it() {
    let got = plugins::with_masters(&world(), &["mygrass.ESP".into()]);
    assert_eq!(got, vec!["Morrowind.esm", "MyGrass.esp"]);
}

#[test]
fn a_name_nothing_in_the_order_matches_is_simply_dropped() {
    let got = plugins::with_masters(&world(), &["MyGrass.esp".into(), "Imaginary.esp".into()]);
    assert_eq!(got, vec!["Morrowind.esm", "MyGrass.esp"]);
}

// ---- against the real writer, not only against my own fixtures --------------------------

#[test]
fn it_reads_a_header_the_engines_own_writer_produced() {
    /* The fixtures above are my reading of the format; this is the format as the rest of
       the program actually writes it. If `plugin_head` and `espwrite` ever disagree about
       where the description ends and the masters begin, Gardenfell's own generated plugin
       is the first thing the chooser would misread. */
    let mut w = gardenfell_tes3::espwrite::Writer::with_capacity(1024);
    let masters = vec![("Morrowind.esm".to_string(), 79_000_000u64), ("Tamriel_Data.esm".to_string(), 1234u64)];
    w.header_with_masters(7, "Gardenfell", "grass for the Bitter Coast", &masters);
    let bytes = w.out;
    let h = esp::plugin_head(&bytes).expect("the engine's own plugin should read");
    assert_eq!(h.masters, vec!["morrowind.esm", "tamriel_data.esm"]);
    assert_eq!(h.desc, "grass for the Bitter Coast");
}

#[test]
fn a_written_plugin_with_no_masters_reads_as_having_none() {
    let mut w = gardenfell_tes3::espwrite::Writer::with_capacity(512);
    w.header(0, "Gardenfell", "standalone");
    let bytes = w.out;
    let h = esp::plugin_head(&bytes).expect("should read");
    assert!(h.masters.is_empty());
    assert_eq!(h.desc, "standalone");
}

#[test]
fn a_master_named_only_by_another_master_is_still_pulled_in() {
    /* The transitive case, properly. Real Morrowind plugins list every master they rest
       on — `TR_Preview.esp` above names all three of its — so the flat shape is the common
       one and a reader that only ever looked one step down would pass the test above. This
       is the chain that would catch it: each link names only the one below. */
    let all = vec![entry("Base.esm", &[]), entry("Mid.esm", &["Base.esm"]), entry("Top.esp", &["Mid.esm"])];
    assert_eq!(plugins::with_masters(&all, &["Top.esp".into()]), vec!["Base.esm", "Mid.esm", "Top.esp"]);
}

#[test]
fn a_four_link_chain_resolves_the_whole_way() {
    let all = vec![
        entry("A.esm", &[]),
        entry("B.esm", &["A.esm"]),
        entry("C.esm", &["B.esm"]),
        entry("D.esp", &["C.esm"]),
    ];
    assert_eq!(plugins::with_masters(&all, &["D.esp".into()]), vec!["A.esm", "B.esm", "C.esm", "D.esp"]);
}
