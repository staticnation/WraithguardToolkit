//! Morrowind's archives — round 11 item 10.
//!
//! Robin: "We need to be able to read BSA archives to showcase a modded load out if
//! someone has work packed in a BSA (uncommon nowadays, but it still happens)." It
//! happens in the base game every day: `Morrowind.bsa` is the only place
//! `Tx_straw_woven.dds` exists, which is why a vanilla basket drew white — the mesh was
//! loose, its texture never was.
//!
//! **The format** (version `0x100`, the only one Morrowind uses) is small:
//!
//! ```text
//!   u32 version        0x00000100
//!   u32 hash_offset    bytes from the end of this 12-byte header to the hash table
//!   u32 count
//!   count × { u32 size, u32 offset }      file records; offset is into the data block
//!   count × u32                           name offsets into the name block
//!   name block                            NUL-terminated names, backslash paths
//!   count × u64                           hashes — not needed, the names are right here
//!   data block                            at 12 + hash_offset + 8 × count
//! ```
//!
//! **Opened, not read.** [`Bsa::open`] reads the header and the tables - the index - and
//! keeps the file open; an entry's bytes are read from the file when asked for
//! ([`Bsa::read`], a positioned read, so threads share the handle). The archive used to be
//! read whole into memory and kept: half a gigabyte for the vanilla three, which a lookup
//! repaid, but a modded install lists many more archives (Tamriel Data and Project
//! Cyrodiil alone are gigabytes), all of them resident at once, and a machine that runs
//! out of memory pages every lookup. [`Bsa::parse`] still holds an archive given as bytes.
//! Archives are opened once per connect and shared through the same `Arc` the overlay is.
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
//! differ in bytes. One of them is a **mesh** — `meshes\x\ex_dae_wall_256_04.nif`, in
//! `Morrowind.bsa` without a `RootCollisionNode` and in `Bloodmoon.bsa` with one — so which
//! archive won decided whether a Daedric wall kept grass out by a 260-triangle visible
//! shape or a 48-triangle hull up to 24 units narrower. That reaches the exported plugin,
//! not just the preview.
//!
//! The one archive no listing names — `Morrowind.bsa`, which the engine loads hardcoded —
//! goes **first**, which is where the engine loads it and so the lowest priority of all;
//! see [`archives_loaded`].

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One open archive.
pub struct Bsa {
    pub path: PathBuf,
    store: Store,
    /// Lowercase forward-slash path → (offset within the archive, size, entry number).
    index: HashMap<String, (usize, usize, usize)>,
    /// Every name as the archive spells it, for listings.
    names: Vec<String>,
}

/// Where an archive's entries are.
enum Store {
    /// All of it, in memory ([`Bsa::parse`]).
    Mem(Vec<u8>),
    /// The open file, read from on demand ([`Bsa::open`]); `kept` holds the entries
    /// [`Bsa::get`] has lent out, by entry number.
    File { file: File, kept: Vec<OnceLock<Vec<u8>>> },
}

/// `buf.len()` bytes of `file` from `at`, without moving a shared cursor: threads read one
/// handle at once.
fn read_at(file: &File, buf: &mut [u8], at: u64) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, at)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < buf.len() {
            let n = file.seek_read(&mut buf[done..], at + done as u64)?;
            if n == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            done += n;
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = file.try_clone()?;
        f.seek(SeekFrom::Start(at))?;
        f.read_exact(buf)
    }
}

impl Bsa {
    /// The archive at `path`: its header and tables read, the file kept open for the rest.
    pub fn open(path: &Path) -> Result<Bsa, String> {
        let err = |e: std::io::Error| format!("{}: {}", path.display(), e);
        let file = File::open(path).map_err(err)?;
        let total = file.metadata().map_err(err)?.len() as usize;
        let mut head = [0u8; 12];
        read_at(&file, &mut head, 0).map_err(|_| format!("{}: truncated at byte 0", path.display()))?;
        let hash_off = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let count = u32::from_le_bytes([head[8], head[9], head[10], head[11]]) as usize;
        // Everything before the data block: the header, the records, the names, the hashes.
        let tables_len = (12 + hash_off).saturating_add(count.saturating_mul(8)).min(total);
        let mut tables = vec![0u8; tables_len];
        read_at(&file, &mut tables, 0).map_err(err)?;
        let (index, names) = Self::tables(path, &tables, total)?;
        let kept = (0..names.len()).map(|_| OnceLock::new()).collect();
        Ok(Bsa { path: path.to_path_buf(), store: Store::File { file, kept }, index, names })
    }

    /// An archive held as bytes.
    pub fn parse(path: &Path, data: Vec<u8>) -> Result<Bsa, String> {
        let (index, names) = Self::tables(path, &data, data.len())?;
        Ok(Bsa { path: path.to_path_buf(), store: Store::Mem(data), index, names })
    }

    /// The index from the archive's first bytes (`data`: at least up to the data block),
    /// with `total` the whole archive's length, which an entry must lie within.
    #[allow(clippy::type_complexity)]
    fn tables(path: &Path, data: &[u8], total: usize) -> Result<(HashMap<String, (usize, usize, usize)>, Vec<String>), String> {
        let u32at = |o: usize| -> Result<u32, String> {
            data.get(o..o + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or_else(|| format!("{}: truncated at byte {}", path.display(), o))
        };
        if u32at(0)? != 0x100 {
            return Err(format!("{}: not a Morrowind archive (version {:#x})", path.display(), u32at(0)?));
        }
        let hash_off = u32at(4)? as usize;
        let count = u32at(8)? as usize;
        let recs = 12;
        let name_offs = recs + 8 * count;
        let names_at = name_offs + 4 * count;
        let data_at = 12 + hash_off + 8 * count;
        if data_at > total || names_at > data.len() {
            return Err(format!("{}: header points past the end of the file", path.display()));
        }
        let mut index = HashMap::with_capacity(count);
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let size = u32at(recs + 8 * i)? as usize;
            let off = u32at(recs + 8 * i + 4)? as usize;
            let no = u32at(name_offs + 4 * i)? as usize;
            let start = names_at + no;
            /* Round 18bc (B3): a name offset past the end skips its record, the way a
               *data* offset past the end already did just below. It used to clamp the
               slice's start for the search and then index `&data[start..end]` with the
               unclamped start, which panicked the whole connect on one damaged archive —
               against this module's own contract, that "a missing or damaged archive is a
               warning on the connect report, not a reason to come up without an
               install". */
            if start > data.len() {
                continue;
            }
            let end = data[start..].iter().position(|&b| b == 0).map(|p| start + p).unwrap_or(data.len());
            /* Round 18gf (backend-plan item 1): the same decoding as a record's path, not
            UTF-8. An archive built from a mod with a non-ASCII letter in a file name
            gave that name a U+FFFD here and the plugin's own spelling of it a Latin-1
            char, so the two never matched and the mesh read as missing. Round 18gh: a
            record's path comes through the typed model now, which decodes Windows-1252
            (`bytes::decode`), so this does the same. Bytes are bytes on both sides. */
            let name = crate::bytes::decode(&data[start..end]);
            let at = data_at + off;
            if at + size > total {
                // A record past the end is a damaged archive; skip the record, keep the rest.
                continue;
            }
            index.insert(crate::paths::norm(&name), (at, size, names.len()));
            names.push(name);
        }
        Ok((index, names))
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Whether the archive holds `rel` (already normalised by [`crate::paths::norm`]).
    pub fn has(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// The bytes of one entry, by normalised key, read for the caller: nothing is kept.
    /// This is what the overlay loads through (`Vfs::load`).
    pub fn read(&self, key: &str) -> Option<Vec<u8>> {
        let &(at, size, _) = self.index.get(key)?;
        match &self.store {
            Store::Mem(d) => Some(d[at..at + size].to_vec()),
            Store::File { file, .. } => {
                let mut b = vec![0u8; size];
                read_at(file, &mut b, at as u64).ok()?;
                Some(b)
            }
        }
    }

    /// The size of one entry, by normalised key, without reading it.
    pub fn size(&self, key: &str) -> Option<usize> {
        self.index.get(key).map(|&(_, size, _)| size)
    }

    /// `len` bytes of one entry from `start` (fewer at its end), by normalised key, read
    /// for the caller. A reader that needs only part of a file - a DDS's smaller mip
    /// levels, after its header - reads that part and not the rest.
    pub fn read_range(&self, key: &str, start: usize, len: usize) -> Option<Vec<u8>> {
        let &(at, size, _) = self.index.get(key)?;
        let start = start.min(size);
        let len = len.min(size - start);
        match &self.store {
            Store::Mem(d) => Some(d[at + start..at + start + len].to_vec()),
            Store::File { file, .. } => {
                let mut b = vec![0u8; len];
                read_at(file, &mut b, (at + start) as u64).ok()?;
                Some(b)
            }
        }
    }

    /// The bytes of one entry, by normalised key, lent. From a file, an entry is read the
    /// first time it is asked for and kept for the archive's life - fine for a tool or a
    /// test that looks at a few; [`Bsa::read`] is the one for loading many.
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        let &(at, size, slot) = self.index.get(key)?;
        match &self.store {
            Store::Mem(d) => Some(&d[at..at + size]),
            Store::File { kept, .. } => {
                let b = kept[slot].get_or_init(|| self.read(key).unwrap_or_default());
                (b.len() == size).then_some(b.as_slice())
            }
        }
    }

    /// Every entry name as the archive spells it.
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
    // An openmw.cfg is its whole chain (round 18gn): the install's file is where the
    // launcher's defaults keep `fallback-archive=Morrowind.bsa`.
    let text = crate::openmwcfg::order_text(order_file);
    if text.is_empty() {
        return (OrderForm::Neither, Vec::new());
    }
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

/// The archives a load order names, in the order it names them.
pub fn archives_named(order_file: &Path) -> Vec<String> {
    archives_listed(order_file).1
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
    let want = crate::paths::norm(want);
    move |have: &String| crate::paths::norm(have) == want
}
