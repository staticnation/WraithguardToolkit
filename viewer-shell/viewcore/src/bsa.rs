//! Morrowind's archives — round 11 item 10.
//!
//! Robin: "We need to be able to read BSA archives to showcase a modded load out if
//! someone has work packed in a BSA (uncommon nowadays, but it still happens)." It
//! happens in the base game every day: `Morrowind.bsa` is the only place
//! `Tx_straw_woven.dds` exists, which is why a vanilla basket drew white — the mesh was
//! loose, its texture never was.
//!
//! The archives are read by greatness7's `tes3::bsa` (see [`Bsa`]): it maps the file,
//! validates it, and hands back each file's bytes on request.
//!
//! **Priority.** A loose file always beats an archive — that is the game's rule and the
//! whole reason mods ship loose. Among archives the load order's own listing decides, and
//! the **last** one loaded wins: archives are opened in the order the listing presents
//! them and a later one overrides an earlier one, the same way a later plugin overrides an
//! earlier plugin. OpenMW says so of its own loader — "if the same filename is contained in
//! multiple archives, the last added archive will have priority" — and Robin's ruling is
//! that Gardenfell follows the listing for both forms.
//!
//! Round 18bs corrected this. It used to be first-named-wins, justified in this very
//! comment as "how the vanilla three never conflict", and that justification was measured
//! and found false: of 60 keys present in more than one of the three vanilla archives, 51
//! differ in bytes. One of them is a **mesh**
//! (`meshes\x\ex_dae_wall_256_04.nif`, with a collision hull only in `Bloodmoon.bsa`), so
//! which archive wins changes what is drawn and what collides.
//!
//! The one archive no listing names — `Morrowind.bsa`, which the engine loads hardcoded —
//! goes **first**, which is where the engine loads it and so the lowest priority of all;
//! see [`archives_loaded`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One open archive, read by greatness7's `tes3::bsa` over a memory map of the file:
/// the archive is not read into memory - only the files asked for are touched - and the
/// crate validates the header, tables and hashes (a damaged archive is refused whole,
/// reported like a missing one).
pub struct Bsa {
    pub path: PathBuf,
    archive: tes3::bsa::Archive<'static>,
    /// Normalised key (see [`crate::vfs::norm`]) -> the name as the archive stores it,
    /// which is what the crate looks a file up by (its hash).
    index: HashMap<String, Vec<u8>>,
    names: Vec<String>,
}

impl Bsa {
    pub fn open(path: &Path) -> Result<Bsa, String> {
        let archive = std::panic::catch_unwind(|| tes3::bsa::Archive::from_path(path))
            .map_err(|_| format!("{}: not a readable Morrowind archive", path.display()))?
            .map_err(|e| format!("{}: {}", path.display(), e))?;
        let mut index = HashMap::with_capacity(archive.len());
        let mut names = Vec::with_capacity(archive.len());
        for entry in archive.entries() {
            let Some(raw) = entry.name() else { continue };
            let name = String::from_utf8_lossy(raw).into_owned();
            index.insert(crate::vfs::norm(&name), raw.to_vec());
            names.push(name);
        }
        Ok(Bsa { path: path.to_path_buf(), archive, index, names })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn has(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<&[u8]> {
        let raw = self.index.get(key)?;
        self.archive.get(raw).map(|e| e.as_bytes())
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// Which kind of file named the archives.
///
/// The two forms are not interchangeable, which is the whole reason this is reported
/// rather than thrown away: an `openmw.cfg` lists **every** archive its engine loads,
/// and a `Morrowind.ini` lists every archive **except** the base game's. See
/// [`archives_loaded`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrderForm {
    /// A `Morrowind.ini` `[Archives]` block — `Archive 0=Tribunal.bsa`.
    MorrowindIni,
    /// An `openmw.cfg` — `fallback-archive=Morrowind.bsa` lines.
    OpenmwCfg,
    /// The file named no archives at all.
    Neither,
}

/// The archive the Morrowind engine loads whether or not anything names it.
pub const IMPLIED_BY_THE_ENGINE: &str = "Morrowind.bsa";

/// The archives a load order names, in the order it names them, and which form named
/// them.
///
/// Both are read from the same file the plugin order came from, so an MO2 profile's own
/// ini answers for that profile. The names are bare filenames resolved through the
/// overlay, because a mod can ship an archive too.
///
/// This is what the file **says**. For what the engine actually opens, which is not the
/// same thing, use [`archives_loaded`].
pub fn archives_listed(order_file: &Path) -> (OrderForm, Vec<String>) {
    let Ok(text) = std::fs::read_to_string(order_file) else { return (OrderForm::Neither, Vec::new()) };
    let mut indexed: Vec<(usize, String)> = Vec::new();
    let mut fallback: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("fallback-archive=") {
            fallback.push(rest.trim().to_string());
        } else if let Some(rest) = t.strip_prefix("Archive ") {
            if let Some(eq) = rest.find('=') {
                if let Ok(n) = rest[..eq].trim().parse::<usize>() {
                    let v = rest[eq + 1..].trim();
                    if !v.is_empty() {
                        indexed.push((n, v.to_string()));
                    }
                }
            }
        }
    }
    if !fallback.is_empty() {
        return (OrderForm::OpenmwCfg, fallback);
    }
    if indexed.is_empty() {
        return (OrderForm::Neither, Vec::new());
    }
    indexed.sort_by_key(|(n, _)| *n);
    (OrderForm::MorrowindIni, indexed.into_iter().map(|(_, s)| s).collect())
}


/* Round 18br (J1): the archive nobody names.

   Reported by a tester, as "no outer walls around Old Ebonheart in the tool, and the
   walls are there in my game". His connect report said **two** archives read — Tribunal
   and Bloodmoon — and listed `Ex_imp_wall_01.NIF`, `Ex_imp_wall_corner_01/02`,
   `Ex_imp_wall_arch_01`, `Ex_imp_wall_stairs_01/02`, `Ex_imp_wallent_02` and the whole
   `Ex_imp_tower*` set among its missing meshes. Which *is* Old Ebonheart's outer wall,
   and every one of those files lives in `Morrowind.bsa`. The walls were missing for the
   plainest possible reason: nothing had opened the archive they are in.

   `archives_listed` above trusts `[Archives]` as the complete list. It is not. The
   Morrowind engine loads `Morrowind.bsa` **unconditionally** — it is hardcoded, not
   driven by the ini — and `[Archives]` names only the archives *besides* it. So the file
   Bethesda ships reads

       [Archives]
       Archive 0=Tribunal.bsa
       Archive 1=Bloodmoon.bsa

   and a reader that opens exactly what it finds there opens two archives out of three.
   `Vfs::add_archives_named_by` did have a fallback for a base game going missing — every
   `.bsa` in the overlay when the listing names none — but it only fires on an **empty**
   list, and the real-world case is a *partial* one.

   `openmw.cfg` is a different matter and is left alone: it lists
   `fallback-archive=Morrowind.bsa` itself, so its listing really is complete, and an
   OpenMW install is entitled to have no `Morrowind.bsa` at all.

   Why this survived to a tester, both halves checked against Robin's own machine: MO2
   *writes* `Archive 0=Morrowind.bsa` into a profile's ini, so his profile names it; and
   every one of those wall meshes is also present loose in his `Data Files\Meshes\x\`, so
   even unopened the archive is not missed. It takes a stock-shaped ini **and** the
   vanilla assets still packed. The tester's Outlander install is both.

   It also did not cost only walls. Those meshes produced no collision hull, so avoidance
   tested against nothing for them — the same failure the note on `add_archives_named_by`
   describes from the other direction — and every plugin exported from such an install has
   grass through Old Ebonheart's walls and inside Dagon Fel's houses.

   And the test that should have caught it wrote an ini that *did* list `Morrowind.bsa`:
   the fixture was the assumption. It is a stock ini now, and the old shape is kept beside
   it as the case where the implicit one must **not** be added twice. */
/// What [`archives_loaded`] answers.
pub struct Loaded {
    /// The archives to open, **in load order** — first loaded first, and so lowest
    /// priority first, because a later archive overrides an earlier one.
    pub names: Vec<String>,
    /// The ones in `names` that nothing in the file named; the engine loads them anyway.
    /// Empty for an `openmw.cfg`, and empty for an ini that names its own base archive.
    ///
    /// The caller needs these by name rather than by position, so that moving where an
    /// implied archive is inserted cannot quietly change which absences get warned about.
    /// It is the difference between "the load order asked for an archive the install
    /// lacks", which is worth saying, and "there is no base archive here", which on a
    /// loose-only tree is not.
    pub implied: Vec<String>,
}

/// The archives the engine actually opens for this load order, in load order.
///
/// [`archives_listed`] is what the file says; this is what the game does. For a
/// `Morrowind.ini` that means [`IMPLIED_BY_THE_ENGINE`] as well, because the engine loads
/// it hardcoded and the `[Archives]` block names only the rest — and **first**, which is
/// when the engine loads it and therefore the lowest priority of all, since everything
/// loaded after overrides it. An ini that names it anyway (MO2 writes one) keeps its own
/// position and gets no duplicate.
///
/// An `openmw.cfg` is returned unchanged: it lists every archive its engine loads.
pub fn archives_loaded(order_file: &Path) -> Loaded {
    let (form, mut names) = archives_listed(order_file);
    let mut implied = Vec::new();
    if form == OrderForm::MorrowindIni && !names.iter().any(same_archive(IMPLIED_BY_THE_ENGINE)) {
        names.insert(0, IMPLIED_BY_THE_ENGINE.to_string());
        implied.push(IMPLIED_BY_THE_ENGINE.to_string());
    }
    Loaded { names, implied }
}

/// The order the base game's archives are loaded in, which is the order they override each
/// other in: an expansion's asset beats the base game's.
pub const VANILLA_ORDER: [&str; 3] = ["Morrowind.bsa", "Tribunal.bsa", "Bloodmoon.bsa"];

/// Whichever of [`VANILLA_ORDER`] appear in `loaded`, out of the order the game loads them
/// in — or `None` when they are in order, which includes any subset of them and any number
/// of other archives anywhere in the list.
///
/// Robin's rule: "We should always load the .bsa files in the order they are presented to us
/// through MO2 settings, morrowind.ini or OpenMW.cfg. The expected order should be
/// Morrowind.bsa, tribunal.bsa, bloodmoon.bsa, and if that is not true print a warning in
/// the report." So the listing is followed as given and the expectation is *reported* rather
/// than silently corrected — a listing that disagrees with the game is a fact about the
/// install, and reordering it here would hide exactly the thing worth knowing.
///
/// Only the three are checked, and only against each other: a mod's own archive belongs
/// wherever the listing puts it, and warning about those would fire on every modded install
/// that ships one.
pub fn vanilla_out_of_order(loaded: &[String]) -> Option<String> {
    let mut seen: Vec<(usize, &str)> = Vec::new();
    for want in VANILLA_ORDER.iter().enumerate() {
        let (rank, name) = want;
        if let Some(at) = loaded.iter().position(same_archive(name)) {
            seen.push((at, VANILLA_ORDER[rank]));
        }
    }
    let mut by_load = seen.clone();
    by_load.sort_by_key(|(at, _)| *at);
    if by_load.iter().map(|(_, n)| *n).eq(seen.iter().map(|(_, n)| *n)) {
        return None;
    }
    Some(by_load.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(", "))
}

/// Whether two archive names are the same file, by the overlay's own spelling rules — so
/// `MORROWIND.BSA`, `Morrowind.bsa` and `Data Files\Morrowind.bsa` are one archive.
fn same_archive(want: &str) -> impl Fn(&String) -> bool + '_ {
    let want = crate::vfs::norm(want);
    move |have: &String| crate::vfs::norm(have) == want
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_archive_the_crate_built() {
        let mut b = tes3::bsa::Builder::new();
        b.insert("Textures\\Tx_A.dds", b"AAAA".to_vec()).unwrap();
        b.insert("meshes\\x\\b.nif", b"BB".to_vec()).unwrap();
        let dir = std::env::temp_dir().join(format!("vcbsa{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.bsa");
        std::fs::write(&p, b.save_bytes().unwrap()).unwrap();
        let a = Bsa::open(&p).unwrap();
        assert_eq!(a.len(), 2);
        assert!(a.has("textures/tx_a.dds"));
        assert_eq!(a.get("textures/tx_a.dds"), Some(&b"AAAA"[..]));
        assert_eq!(a.get("meshes/x/b.nif"), Some(&b"BB"[..]));
        assert_eq!(a.get("nope"), None);
        assert_eq!(a.names().len(), 2);
        drop(a);
        std::fs::write(&p, b"not an archive").unwrap();
        assert!(Bsa::open(&p).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
