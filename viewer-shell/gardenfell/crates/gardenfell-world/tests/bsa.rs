//! Archives — round 11 item 10.
//!
//! `fixture/packed.bsa` is a three-file archive in the real format, written by hand so
//! the test runs anywhere. The vanilla archives, when they are present beside this
//! checkout (`GARDENFELL_BSA` names one), are read as well, because a format guessed
//! from a spec and a format the game actually wrote are two different things until the
//! second has been opened.

use gardenfell_bsa::bsa::{archives_listed, archives_loaded, archives_named, Bsa, OrderForm};
use gardenfell_install::vfs::{Loc, Vfs};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!("{}/../../fixture/{}", env!("CARGO_MANIFEST_DIR"), name))
}

#[test]
fn an_archive_is_read_and_its_entries_found_by_normalised_name() {
    let a = Bsa::open(&fixture("packed.bsa")).expect("opens");
    assert_eq!(a.len(), 3);
    assert_eq!(a.get("textures/tx_packed.dds"), Some(&b"DDS packed bytes"[..]));
    assert_eq!(a.get("meshes/m/packed.nif"), Some(&b"NIF packed"[..]));
    assert!(a.has("meshes/m/both.nif"));
    assert!(!a.has("meshes/m/loose.nif"));
    assert!(a.names().iter().any(|n| n == "textures\\tx_packed.dds"), "names as the archive spells them");
}

#[test]
fn loose_files_win_over_the_archive_and_the_archive_fills_the_rest() {
    /* The game's rule, and the reason mods ship loose: a file on disk beats the same
       name in any archive. Everything the archive alone has is there too, and the mesh
       picker lists it beside the loose ones. */
    let dir = std::env::temp_dir().join(format!("gf_bsa_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("meshes/m")).unwrap();
    std::fs::write(dir.join("meshes/m/both.nif"), b"from disk").unwrap();
    std::fs::write(dir.join("meshes/m/loose.nif"), b"loose").unwrap();
    let mut v = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    let errs = v.add_archives(&[fixture("packed.bsa")]);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(v.packed_count(), 3);

    assert_eq!(v.read("meshes/m/both.nif").as_deref(), Some(&b"from disk"[..]), "loose wins");
    assert_eq!(v.read("meshes/m/packed.nif").as_deref(), Some(&b"NIF packed"[..]), "packed fills in");
    assert!(matches!(v.resolve_mesh("m/packed.nif"), Some(Loc::Packed { .. })));
    assert!(matches!(v.resolve_mesh("m/both.nif"), Some(Loc::Disk(_))));
    assert!(v.resolve_disk("meshes/m/packed.nif").is_none(), "a packed file is not a disk path");
    // The texture family lookup reaches into the archive too, `.tga` asked and `.dds` found.
    assert!(matches!(v.resolve_texture("tx_packed.tga"), Some(Loc::Packed { .. })));

    let listed = v.list("meshes/", &[".nif"]);
    assert!(listed.iter().any(|n| n.eq_ignore_ascii_case("meshes/m/packed.nif")), "{listed:?}");
    assert_eq!(listed.iter().filter(|n| n.to_ascii_lowercase().ends_with("both.nif")).count(), 1, "one row for a name both have");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_archives_a_load_order_names_are_read_in_its_order() {
    let dir = std::env::temp_dir().join(format!("gf_bsa_ini_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    /* Round 18br (J1): a **stock** ini, which is the shape Bethesda ships and the shape
       this fixture used to get wrong. It does not name `Morrowind.bsa`, because the engine
       loads that one hardcoded. `archives_named` answers what the file says — two — and
       `archives_loaded` answers what the engine opens — three. */
    let ini = dir.join("Morrowind.ini");
    std::fs::write(&ini, "[Archives]\nArchive 1=Bloodmoon.bsa\nArchive 0=Tribunal.bsa\n\n[Game Files]\nGameFile0=Morrowind.esm\n").unwrap();
    assert_eq!(archives_named(&ini), vec!["Tribunal.bsa", "Bloodmoon.bsa"], "what the file says");
    assert_eq!(archives_listed(&ini).0, OrderForm::MorrowindIni);

    let ini_mo2 = dir.join("MO2.ini");
    std::fs::write(&ini_mo2, "[Archives]\nArchive 1=Tribunal.bsa\nArchive 0=Morrowind.bsa\nArchive 2=Bloodmoon.bsa\n").unwrap();
    assert_eq!(archives_named(&ini_mo2), vec!["Morrowind.bsa", "Tribunal.bsa", "Bloodmoon.bsa"]);

    let cfg = dir.join("openmw.cfg");
    std::fs::write(&cfg, "fallback-archive=Morrowind.bsa\nfallback-archive=Tribunal.bsa\ncontent=Morrowind.esm\n").unwrap();
    assert_eq!(archives_named(&cfg), vec!["Morrowind.bsa", "Tribunal.bsa"]);
    assert_eq!(archives_listed(&cfg).0, OrderForm::OpenmwCfg);

    assert!(archives_named(Path::new("/nowhere/at/all.ini")).is_empty());
    assert_eq!(archives_listed(Path::new("/nowhere/at/all.ini")).0, OrderForm::Neither);
    let _ = std::fs::remove_dir_all(&dir);
}

/* ---- Round 18br (J1): the archive nobody names ----------------------------------- */

#[test]
fn j1_a_stock_morrowind_ini_still_loads_the_base_archive() {
    /* The tester's case. A stock `[Archives]` block names the two expansion archives and
       not `Morrowind.bsa`, because the game loads that one whether the ini mentions it or
       not — so a reader that opens exactly what the block lists opens two archives out of
       three, and every base-game mesh that no mod replaced loose reads as missing. His
       were `Ex_imp_wall_*` and `Ex_imp_tower*`: Old Ebonheart's outer walls. */
    let dir = std::env::temp_dir().join(format!("gf_j1_ini_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    std::fs::write(&ini, "[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Bloodmoon.bsa\n").unwrap();

    let got = archives_loaded(&ini);
    /* Round 18bs: **first**, which is when the engine loads it, and therefore the lowest
       priority of all now that a later archive overrides an earlier one. Robin: "The
       expected order should be Morrowind.bsa, tribunal.bsa, bloodmoon.bsa." */
    assert_eq!(got.names, vec!["Morrowind.bsa", "Tribunal.bsa", "Bloodmoon.bsa"]);
    assert_eq!(got.implied, vec!["Morrowind.bsa"], "the base archive, which nothing named");
    assert!(gardenfell_bsa::bsa::vanilla_out_of_order(&got.names).is_none(), "and so in the expected order");

    // An ini that names it anyway keeps its own position and gets no duplicate.
    let mo2 = dir.join("MO2.ini");
    std::fs::write(&mo2, "[Archives]\nArchive 0=Morrowind.bsa\nArchive 1=Tribunal.bsa\n").unwrap();
    let got = archives_loaded(&mo2);
    assert_eq!(got.names, vec!["Morrowind.bsa", "Tribunal.bsa"]);
    assert!(got.implied.is_empty());

    // …in any spelling, because the overlay does not care about case or separators.
    for spelling in ["MORROWIND.BSA", "morrowind.bsa", r"Data Files\Morrowind.bsa"] {
        let p = dir.join("spelled.ini");
        std::fs::write(&p, format!("[Archives]\nArchive 0={spelling}\nArchive 1=Tribunal.bsa\n")).unwrap();
        let got = archives_loaded(&p);
        assert!(got.implied.is_empty(), "{spelling} is the base archive");
        assert_eq!(got.names.len(), 2, "{spelling} was added twice");
    }

    /* An openmw.cfg lists every archive its engine loads, and an OpenMW install is
       entitled to have no `Morrowind.bsa` at all — so nothing is added there. */
    let cfg = dir.join("openmw.cfg");
    std::fs::write(&cfg, "fallback-archive=Tribunal.bsa\ncontent=Morrowind.esm\n").unwrap();
    let got = archives_loaded(&cfg);
    assert_eq!(got.names, vec!["Tribunal.bsa"]);
    assert!(got.implied.is_empty(), "openmw.cfg's listing is complete");

    // A file that names none keeps answering none, so the overlay-wide fallback still runs.
    let bare = dir.join("bare.ini");
    std::fs::write(&bare, "[Game Files]\nGameFile0=Morrowind.esm\n").unwrap();
    assert!(archives_loaded(&bare).names.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Morrowind archive, version 0x100, built from names and bytes so a test can have two.
fn mini_bsa(files: &[(&str, &[u8])]) -> Vec<u8> {
    let count = files.len();
    let name_block: Vec<u8> = files.iter().flat_map(|(n, _)| {
        let mut v = n.as_bytes().to_vec();
        v.push(0);
        v
    }).collect();
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(&0x100u32.to_le_bytes());
    // The hash table sits after the records, the name offsets and the names.
    b.extend_from_slice(&((12 * count + name_block.len()) as u32).to_le_bytes());
    b.extend_from_slice(&(count as u32).to_le_bytes());
    let mut at = 0u32;
    for (_, d) in files {
        b.extend_from_slice(&(d.len() as u32).to_le_bytes());
        b.extend_from_slice(&at.to_le_bytes());
        at += d.len() as u32;
    }
    let mut no = 0u32;
    for (n, _) in files {
        b.extend_from_slice(&no.to_le_bytes());
        no += n.len() as u32 + 1;
    }
    b.extend_from_slice(&name_block);
    b.extend_from_slice(&vec![0u8; 8 * count]); // hashes — the names are right there
    for (_, d) in files {
        b.extend_from_slice(d);
    }
    b
}

#[test]
fn j1_the_base_archive_loses_to_every_archive_the_listing_names() {
    /* The priority half, pinned as behaviour rather than as a position in a list: whatever
       order the listing is walked in, an entry a mod's archive or an expansion also has
       must not come out of `Morrowind.bsa`. The engine loads that one before the
       `[Archives]` block, so the block overrides it.
       This test is why round 18bs was safe: written this way, it fails if the base archive
       is left at the tail when the rule flips to last-wins — which it was, and it did. */
    let dir = std::env::temp_dir().join(format!("gf_j1_pri_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Morrowind.bsa"), mini_bsa(&[
        (r"meshes\x\ex_imp_wall_01.nif", b"base wall"),
        (r"meshes\x\shared.nif", b"from the base game"),
    ])).unwrap();
    std::fs::write(dir.join("Tribunal.bsa"), mini_bsa(&[
        (r"meshes\x\shared.nif", b"from Tribunal"),
    ])).unwrap();
    let ini = dir.join("Morrowind.ini");
    std::fs::write(&ini, "[Archives]\nArchive 0=Tribunal.bsa\n").unwrap();

    let mut v = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    let warnings = v.add_archives_named_by(&ini);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(v.archives.len(), 2, "the named one and the implied one");

    // The wall exists at all — which is the tester's bug, in one line.
    assert_eq!(v.read(r"meshes\x\ex_imp_wall_01.nif").as_deref(), Some(&b"base wall"[..]));
    // And the base archive loses where something the ini named has the same entry.
    assert_eq!(v.read(r"meshes\x\shared.nif").as_deref(), Some(&b"from Tribunal"[..]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j1_an_install_with_no_base_archive_is_not_warned_about() {
    /* A loose-only tree is legitimate — it is what an OpenMW user or a fully unpacked
       install looks like — so the archive the load order never asked for going missing is
       not a warning. An archive the ini *did* name and the install lacks still is. */
    let dir = std::env::temp_dir().join(format!("gf_j1_warn_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("meshes/x")).unwrap();
    std::fs::write(dir.join("meshes/x/ex_imp_wall_01.nif"), b"loose wall").unwrap();
    std::fs::write(dir.join("Tribunal.bsa"), mini_bsa(&[(r"meshes\x\t.nif", b"t")])).unwrap();

    let ini = dir.join("Morrowind.ini");
    std::fs::write(&ini, "[Archives]\nArchive 0=Tribunal.bsa\n").unwrap();
    let mut v = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    assert!(v.add_archives_named_by(&ini).is_empty(), "no Morrowind.bsa is not a complaint");
    assert_eq!(v.archives.len(), 1);
    assert_eq!(v.read(r"meshes\x\ex_imp_wall_01.nif").as_deref(), Some(&b"loose wall"[..]));

    let ini2 = dir.join("Missing.ini");
    std::fs::write(&ini2, "[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Nothing.bsa\n").unwrap();
    let mut v2 = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    let w = v2.add_archives_named_by(&ini2);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("Nothing.bsa"), "{}", w[0]);
    assert!(!w[0].contains("Morrowind.bsa"), "{}", w[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_real_vanilla_archive_opens_when_one_is_at_hand() {
    /* Not a fixture: the game's own archive, when the environment points at one. Skipped
       quietly otherwise — a format is only proven against the files it has to read. */
    let Ok(p) = std::env::var("GARDENFELL_BSA") else { return };
    let a = Bsa::open(Path::new(&p)).expect("the vanilla archive opens");
    assert!(a.len() > 1000, "{} entries", a.len());
    let nifs = a.names().iter().filter(|n| n.to_ascii_lowercase().ends_with(".nif")).count();
    let dds = a.names().iter().filter(|n| n.to_ascii_lowercase().ends_with(".dds")).count();
    assert!(nifs > 100 && dds > 100, "{nifs} meshes, {dds} textures");
    // Every entry's bytes are reachable and the sizes are what the records said.
    for n in a.names().iter().take(200) {
        let b = a.get(&gardenfell_install::vfs::norm(n)).expect("entry readable");
        if n.to_ascii_lowercase().ends_with(".dds") {
            assert_eq!(&b[..4], b"DDS ", "{n} is not a DDS at its recorded offset");
        }
        if n.to_ascii_lowercase().ends_with(".nif") {
            assert!(b.starts_with(b"NetImmerse") || b.starts_with(b"Gamebryo"), "{n} is not a NIF at its recorded offset");
        }
    }
}

/* ---- Round 18bs (J2): last loaded wins, and an unexpected order is reported --------- */

#[test]
fn j2_a_later_archive_overrides_an_earlier_one() {
    /* Robin's ruling: follow the listing, and a later archive overrides an earlier one —
       OpenMW's own rule, and the same direction as plugin load order. It used to be the
       first-named that won. */
    let dir = std::env::temp_dir().join(format!("gf_j2_last_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Morrowind.bsa"), mini_bsa(&[
        (r"meshes\x\base_only.nif", b"only the base game has this"),
        (r"meshes\x\shared.nif", b"the base game's version"),
    ])).unwrap();
    std::fs::write(dir.join("Tribunal.bsa"), mini_bsa(&[
        (r"meshes\x\shared.nif", b"Tribunal's version"),
    ])).unwrap();
    std::fs::write(dir.join("Bloodmoon.bsa"), mini_bsa(&[
        (r"meshes\x\shared.nif", b"Bloodmoon's version"),
    ])).unwrap();
    let ini = dir.join("Morrowind.ini");
    std::fs::write(&ini, "[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Bloodmoon.bsa\n").unwrap();

    let mut v = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    let warnings = v.add_archives_named_by(&ini);
    assert!(warnings.is_empty(), "the expected order warns about nothing: {warnings:?}");
    assert_eq!(v.archives.len(), 3);

    // Last loaded wins, so the last of the three the listing presents.
    assert_eq!(v.read(r"meshes\x\shared.nif").as_deref(), Some(&b"Bloodmoon's version"[..]));
    // And an archive is still the only place its own entries come from.
    assert_eq!(v.read(r"meshes\x\base_only.nif").as_deref(), Some(&b"only the base game has this"[..]));
    // A loose file still beats every archive — that rule did not move.
    std::fs::create_dir_all(dir.join("meshes/x")).unwrap();
    std::fs::write(dir.join("meshes/x/shared.nif"), b"loose").unwrap();
    let mut v2 = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    v2.add_archives_named_by(&ini);
    assert_eq!(v2.read(r"meshes\x\shared.nif").as_deref(), Some(&b"loose"[..]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j2_an_unexpected_base_game_order_is_reported_not_corrected() {
    use gardenfell_bsa::bsa::{vanilla_out_of_order, VANILLA_ORDER};
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // In order, whole or in part, with anything else anywhere: nothing to say.
    assert!(vanilla_out_of_order(&v(&VANILLA_ORDER)).is_none());
    assert!(vanilla_out_of_order(&v(&["Morrowind.bsa", "Bloodmoon.bsa"])).is_none(), "a subset is in order");
    assert!(vanilla_out_of_order(&v(&["Tribunal.bsa"])).is_none(), "one of them cannot be out of order");
    assert!(vanilla_out_of_order(&v(&[])).is_none());
    /* A mod's archive belongs wherever the listing puts it. Warning about those would fire
       on every modded install that ships one — Robin's own MO2 profile lists
       MW_Children_1_0.bsa. */
    assert!(vanilla_out_of_order(&v(&["Patch.bsa", "Morrowind.bsa", "Tamriel_Data.bsa",
                                      "Tribunal.bsa", "MW_Children_1_0.bsa", "Bloodmoon.bsa"])).is_none());
    // Case and separators are the overlay's business, not this function's.
    assert!(vanilla_out_of_order(&v(&["MORROWIND.BSA", r"Data Files\Tribunal.bsa", "bloodmoon.bsa"])).is_none());

    // Out of order: named, in the order they actually load.
    assert_eq!(vanilla_out_of_order(&v(&["Bloodmoon.bsa", "Tribunal.bsa"])).as_deref(),
               Some("Bloodmoon.bsa, Tribunal.bsa"));
    assert_eq!(vanilla_out_of_order(&v(&["Tribunal.bsa", "Morrowind.bsa", "Bloodmoon.bsa"])).as_deref(),
               Some("Tribunal.bsa, Morrowind.bsa, Bloodmoon.bsa"));

    /* And it reaches the connect report rather than being fixed behind the person's back:
       the listing is followed as given, which is what makes the warning worth having. */
    let dir = std::env::temp_dir().join(format!("gf_j2_warn_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["Morrowind.bsa", "Tribunal.bsa", "Bloodmoon.bsa"] {
        std::fs::write(dir.join(n), mini_bsa(&[(r"meshes\x\a.nif", n.as_bytes())])).unwrap();
    }
    let ini = dir.join("Morrowind.ini");
    // MO2 wrote the base archive in, and somebody put the expansions the wrong way round.
    std::fs::write(&ini, "[Archives]\nArchive 0=Morrowind.bsa\nArchive 1=Bloodmoon.bsa\nArchive 2=Tribunal.bsa\n").unwrap();
    let mut vfs = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    let w = vfs.add_archives_named_by(&ini);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("unexpected order"), "{}", w[0]);
    assert!(w[0].contains("Morrowind.bsa, Bloodmoon.bsa, Tribunal.bsa"), "{}", w[0]);
    assert!(w[0].contains("expected Morrowind.bsa, Tribunal.bsa, Bloodmoon.bsa"), "{}", w[0]);
    // Reported, not corrected: Tribunal is still last, so Tribunal still wins.
    assert_eq!(vfs.read(r"meshes\x\a.nif").as_deref(), Some(&b"Tribunal.bsa"[..]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j2_the_daedric_wall_is_the_case_this_was_measured_on() {
    /* The real file that made J2 worth fixing, when the vanilla archives are at hand:
       `ex_dae_wall_256_04.nif` is in Morrowind.bsa without a `RootCollisionNode` and in
       Bloodmoon.bsa with one, so which archive wins decides whether a Daedric wall keeps
       grass out by its visible geometry or by a hull a good deal narrower. Skipped quietly
       where the archives are not beside the checkout. */
    let Ok(base) = std::env::var("GARDENFELL_DATA") else { return };
    let base = PathBuf::from(base);
    let (mw, bm) = (base.join("Morrowind.bsa"), base.join("Bloodmoon.bsa"));
    if !mw.is_file() || !bm.is_file() {
        return;
    }
    let key = "meshes/x/ex_dae_wall_256_04.nif";
    let a = Bsa::open(&mw).expect("Morrowind.bsa opens");
    let b = Bsa::open(&bm).expect("Bloodmoon.bsa opens");
    let (Some(from_mw), Some(from_bm)) = (a.get(key), b.get(key)) else {
        panic!("both archives were expected to hold {key}");
    };
    assert_ne!(from_mw, from_bm, "the two versions differ — that is the whole finding");

    let mut v = Vfs::with_roots(&[]);
    // Load order, so Bloodmoon is opened after the base game and overrides it.
    let errs = v.add_archives(&[mw.clone(), base.join("Tribunal.bsa"), bm.clone()]);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(v.read(key).as_deref(), Some(from_bm), "the expansion's version is the one read");

    // And the hull that comes out of it is the collision node's, not the visible shape's.
    let got = gardenfell_nif::nif::parse(&v.read(key).unwrap()).expect("reads as a mesh");
    let mw_mesh = gardenfell_nif::nif::parse(from_mw).expect("reads as a mesh");
    assert_ne!(got.tris.len(), mw_mesh.tris.len(),
               "the two give different obstacles: {} vs {} triangles", got.tris.len(), mw_mesh.tris.len());
}
