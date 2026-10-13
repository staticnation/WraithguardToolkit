//! Ordered overlay filesystem.
//!
//! A plain install has one root; an MO2 profile has one per enabled mod plus the
//! game folder; OpenMW has one per `data=` line. Each root is indexed once into a
//! lowercase path map, after which every lookup is a hash probe instead of a
//! directory walk. Morrowind's own paths are case-insensitive and inconsistently
//! cased in the wild, so the index is the only sane way to resolve them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Root {
    pub label: String,
    pub dir: PathBuf,
    /// lowercase relative path (forward slashes) -> real path
    index: HashMap<String, PathBuf>,
}

#[derive(Default)]
pub struct Vfs {
    /// Highest priority first.
    pub roots: Vec<Root>,
    /// The archives the load order names, first named winning — behind every root,
    /// because a loose file always beats an archive (round 11 item 10).
    pub archives: Vec<crate::bsa::Bsa>,
}

/// Where a file is: on disk, or packed in an archive.
///
/// Every `resolve` answers one of these since round 11. Callers that only ask "is it
/// there" are unchanged; callers that read go through [`Vfs::load`], which knows both.
/// A plugin is always on disk, and [`Vfs::resolve_disk`] is the form that says so.
#[derive(Clone, Debug, PartialEq)]
pub enum Loc {
    Disk(PathBuf),
    Packed { archive: usize, key: String },
}

impl Loc {
    /// A stable name for the file, for indexes keyed on where a file came from: the
    /// path on disk, or `archive.bsa::key` for a packed one.
    pub fn ident(&self, vfs: &Vfs) -> String {
        match self {
            Loc::Disk(p) => p.to_string_lossy().to_string(),
            Loc::Packed { archive, key } => format!(
                "{}::{}",
                vfs.archives.get(*archive).map(|a| a.path.to_string_lossy().to_string()).unwrap_or_default(),
                key
            ),
        }
    }
    pub fn is_packed(&self) -> bool {
        matches!(self, Loc::Packed { .. })
    }
}

/// Which place a hit of [`Vfs::family_walk`] came from: a root or an archive, by index.
#[derive(Clone, Copy)]
enum Place {
    Root(usize),
    Archive(usize),
}

// `norm` lives in `crate::paths` (18ge), re-exported so `vfs::norm` still resolves.
pub use crate::paths::norm;

fn index_dir(dir: &Path, prefix: &str, out: &mut HashMap<String, PathBuf>, depth: u32) {
    if depth > 12 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        /* Folded the way `norm` folds a lookup (round 18gq: through the whole alphabet when
           a name has a letter outside ASCII). A plugin read in its own code page now spells
           a Russian mod's `Трава\Трава.nif` as those letters, so a disk name must fold the
           same way or the two never meet. ASCII names, which is nearly all of them, still
           cost one pass. */
        let name = if name.is_ascii() { name.to_ascii_lowercase() } else { name.to_lowercase() };
        let key = if prefix.is_empty() { name } else { format!("{}/{}", prefix, name) };
        /* Round 18bf (I16): a symlink is asked what it points *at*.
           `DirEntry::file_type` does not follow one, so a symlinked directory answered
           "not a directory" and was filed as a file — hiding everything under it. That is
           a Proton install (where the prefix's `drive_c` is routinely a link) or a shared
           asset library on Linux, and the failure is total: the mod's whole tree is
           invisible and every mesh in it reports as missing. `fs::metadata` follows the
           link; the depth cap above is what keeps a loop of them finite. */
        let dir_like = match e.file_type() {
            Ok(ft) if ft.is_dir() => true,
            Ok(ft) if ft.is_symlink() => std::fs::metadata(e.path()).map(|m| m.is_dir()).unwrap_or(false),
            _ => false,
        };
        if dir_like {
            index_dir(&e.path(), &key, out, depth + 1);
            continue;
        }
        /* `.mohidden` is how Mod Organizer takes a file out of the virtual filesystem
           without deleting it. It is not in the overlay the game sees, so it is not in
           this one either — and it must not turn up in an asset browser looking like
           something you could pick. */
        if e.file_type().is_ok() && !key.ends_with(".mohidden") {
            out.entry(key).or_insert_with(|| e.path());
        }
    }
}

impl Vfs {
    pub fn with_roots(dirs: &[(String, PathBuf)]) -> Vfs {
        // Indexing each root is independent, so do them all at once.
        let roots = crate::pool::par_map(dirs, |_, (label, dir)| {
            let mut index = HashMap::new();
            index_dir(dir, "", &mut index, 0);
            Root { label: label.clone(), dir: dir.clone(), index }
        });
        Vfs { roots, archives: Vec::new() }
    }

    pub fn file_count(&self) -> usize {
        self.roots.iter().map(|r| r.index.len()).sum()
    }

    /// Opens the archives at `paths`, in that order of priority. Failures are reported
    /// rather than fatal: a missing or damaged archive is a warning on the connect
    /// report, not a reason to come up without an install.
    pub fn add_archives(&mut self, paths: &[PathBuf]) -> Vec<String> {
        let opened = crate::pool::par_map(paths, |_, p| crate::bsa::Bsa::open(p));
        let mut errs = Vec::new();
        for r in opened {
            match r {
                Ok(b) => self.archives.push(b),
                Err(e) => errs.push(e),
            }
        }
        errs
    }

    /* Round 18bo: the archives, in one place.

       This block used to live only in the window's `open_overlay`, so `gdn_cli` built an
       overlay with **no archives in it at all**: every vanilla mesh read as missing, every
       static failed to produce a collision hull, and avoidance quietly tested against
       nothing. A plugin generated from the shell was therefore not the plugin the window
       writes — the same class of drift `--profile` was added to close in 18bh, and found
       the same way, by running the two side by side and noticing "1 of 1743 meshes loaded".

       So it is a method now, and both front ends call it. Two copies of this reasoning is
       what caused the bug; one copy is the fix. */
    /// Open the archives the load order loads, or every archive the overlay can see when
    /// it names none.
    ///
    /// Named by the same file the plugin order came from — `[Archives]` in a Morrowind.ini,
    /// `fallback-archive=` lines in an openmw.cfg — and resolved through the overlay,
    /// because a mod can ship one. The list comes from [`crate::bsa::archives_loaded`]
    /// rather than `archives_named`, so a Morrowind.ini gets `Morrowind.bsa` too: the
    /// engine loads that one hardcoded and the ini names only the rest, which is round
    /// 18br's bug (J1) and the long note in `bsa.rs` explains it. A load order that names
    /// none at all gets every `.bsa` in the overlay, which is what a Morrowind.ini with its
    /// `[Archives]` block stripped out would otherwise mean: the base game's own meshes and
    /// textures vanishing. Loose files win over every archive; among archives the listing's
    /// order decides.
    ///
    /// Returns what went wrong, as warnings — a missing or damaged archive is something to
    /// say, not a reason to refuse to open an install. The engine's implicit archive is the
    /// one exception: an install with no `Morrowind.bsa` is either an OpenMW-style loose-only
    /// tree, which is legitimate, or broken in a way the "N of M meshes loaded" line already
    /// reports. Warning there would be a false alarm on the first, so it says nothing.
    pub fn add_archives_named_by(&mut self, order_file: &Path) -> Vec<String> {
        let crate::bsa::Loaded { mut names, implied } = crate::bsa::archives_loaded(order_file);
        if names.is_empty() {
            names = self.list("", &[".bsa"]);
            names.sort_by_key(|n| n.to_ascii_lowercase());
        }
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        let mut opened: Vec<String> = Vec::new();
        for n in &names {
            match self.resolve_disk(n) {
                Some(p) => {
                    paths.push(p.to_path_buf());
                    opened.push(n.clone());
                }
                /* An archive nothing named going missing is not the load order failing to
                   find something it asked for. Matched by name rather than by position, so
                   that where an implied archive is inserted cannot change what is warned
                   about — round 18bs moved `Morrowind.bsa` from the tail to the head. */
                None if !implied.iter().any(|i| norm(i) == norm(n)) => missing.push(n.clone()),
                None => {}
            }
        }
        let mut warnings: Vec<String> = Vec::new();
        for e in self.add_archives(&paths) {
            warnings.push(format!("archive not read: {e}"));
        }
        if !missing.is_empty() {
            warnings.push(format!(
                "archive(s) the load order names but the install lacks: {}",
                missing.join(", ")
            ));
        }
        /* Round 18bs, Robin's rule: the listing is followed as it is given, and an
           unexpected order is *said* rather than corrected. The base game's three override
           each other in one direction, so a listing that has them the other way round is a
           fact about the install worth knowing about before its grass is explained. */
        if let Some(actual) = crate::bsa::vanilla_out_of_order(&opened) {
            warnings.push(format!(
                "the base game's archives load in an unexpected order: {} — expected {}, \
                 and a later archive overrides an earlier one",
                actual,
                crate::bsa::VANILLA_ORDER.join(", ")
            ));
        }
        warnings
    }

    /// How many files the archives hold between them.
    pub fn packed_count(&self) -> usize {
        self.archives.iter().map(|a| a.len()).sum()
    }

    /* Round 18bs: the **last** archive that has it, not the first. `archives` is in load
       order, and a later archive overrides an earlier one — OpenMW's own rule ("the last
       added archive will have priority") and the same direction as plugin load order. It
       used to take the first, on a comment claiming the vanilla three never conflict; they
       do, in 51 keys, one of them a mesh whose two versions give different collision
       hulls. See the note at the top of `bsa.rs`. */
    fn packed(&self, key: &str) -> Option<Loc> {
        self.archives
            .iter()
            .rposition(|a| a.has(key))
            .map(|i| Loc::Packed { archive: i, key: key.to_string() })
    }

    /// First root that has the path wins, matching how the game resolves conflicts;
    /// the archives answer only when no root does.
    pub fn resolve(&self, rel: &str) -> Option<Loc> {
        let key = norm(rel);
        for r in &self.roots {
            if let Some(p) = r.index.get(&key) {
                return Some(Loc::Disk(p.clone()));
            }
        }
        self.packed(&key)
    }

    /// Every place that supplies `rel`, in the order [`Vfs::resolve_family`] looks (`exts`
    /// as it reads them; none for the path as spelled): the first is the file
    /// `resolve_family` loads, each one after it the file that would answer were the ones
    /// before it gone. `(label, location)`: the folder's label, or the archive's file name.
    ///
    /// The list and the resolver are one walk ([`Vfs::family_walk`]), so the list cannot
    /// name a file the resolver would not load, whatever order the resolver keeps.
    pub fn providers(&self, rel: &str, exts: &[&str]) -> Vec<(String, Loc)> {
        let mut out: Vec<(String, Loc)> = Vec::new();
        self.family_walk(rel, exts, &mut |from, loc| {
            let label = match from {
                Place::Root(r) => self.roots[r].label.clone(),
                Place::Archive(a) => {
                    self.archives[a].path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                }
            };
            out.push((label, loc));
            false
        });
        out
    }

    /// [`Vfs::providers`] for a texture, looked for as [`Vfs::resolve_texture`] looks: the
    /// `textures/` spelling's places, then the spelling as given.
    pub fn texture_providers(&self, rel: &str) -> Vec<(String, Loc)> {
        let n = norm(rel);
        let mut out = Vec::new();
        if !n.starts_with("textures/") {
            out.extend(self.providers(&format!("textures/{}", n), Self::TEX_EXTS));
        }
        out.extend(self.providers(&n, Self::TEX_EXTS));
        out
    }

    /// [`Vfs::providers`] for a mesh, looked for as [`Vfs::resolve_mesh`] looks.
    pub fn mesh_providers(&self, rel: &str) -> Vec<(String, Loc)> {
        let n = norm(rel);
        let mut out = Vec::new();
        if !n.starts_with("meshes/") {
            out.extend(self.providers(&format!("meshes/{}", n), &[]));
        }
        out.extend(self.providers(&n, &[]));
        out
    }

    /// A loose file only — what a plugin is, always.
    pub fn resolve_disk(&self, rel: &str) -> Option<&Path> {
        let key = norm(rel);
        for r in &self.roots {
            if let Some(p) = r.index.get(&key) {
                return Some(p.as_path());
            }
        }
        None
    }

    /// The bytes behind a location, wherever it is.
    pub fn load(&self, loc: &Loc) -> Option<Vec<u8>> {
        match loc {
            Loc::Disk(p) => std::fs::read(p).ok(),
            // Read for the caller, not lent: an archive keeps nothing it hands out here.
            Loc::Packed { archive, key } => self.archives.get(*archive)?.read(key),
        }
    }

    /// The size of the file at `loc`, without reading it.
    pub fn size(&self, loc: &Loc) -> Option<usize> {
        match loc {
            Loc::Disk(p) => std::fs::metadata(p).ok().map(|m| m.len() as usize),
            Loc::Packed { archive, key } => self.archives.get(*archive)?.size(key),
        }
    }

    /// `len` bytes of the file at `loc` from `start` (fewer at its end): part of a file,
    /// for a reader that needs no more (a DDS's header, then only the mip levels it
    /// keeps). A loose file is seeked, an archive entry read at its offset.
    pub fn load_range(&self, loc: &Loc, start: usize, len: usize) -> Option<Vec<u8>> {
        match loc {
            Loc::Disk(p) => {
                use std::io::{Read, Seek, SeekFrom};
                let mut f = std::fs::File::open(p).ok()?;
                f.seek(SeekFrom::Start(start as u64)).ok()?;
                let mut b = Vec::with_capacity(len.min(1 << 26));
                f.take(len as u64).read_to_end(&mut b).ok()?;
                Some(b)
            }
            Loc::Packed { archive, key } => self.archives.get(*archive)?.read_range(key, start, len),
        }
    }

    /// The extensions a texture may actually be on disk under, `.dds` first.
    ///
    /// Morrowind's own records name `.tga` and the files behind them are frequently
    /// `.dds`, so every one of these is a candidate for a path that says any other one.
    pub const TEX_EXTS: &[&str] = &[".dds", ".tga", ".bmp", ".png", ".jpg"];

    /// The overlay's answer for a path whose extension a mod may have changed.
    ///
    /// **Root-major, extension-minor**, and that order is the whole point. Resolving
    /// extension-first — every root for `.tga`, then every root for `.dds` — asks the
    /// wrong question: it lets a *lower priority* mod win as long as it happens to spell
    /// the name the record used. That is why Robin's landscape looked vanilla under a
    /// stack of retextures. Nearly every land retexture ships `tx_ac_rock_01.dds` against
    /// a record that says `Tx_AC_rock_01.tga`, so vanilla's loose `.tga` — bottom of the
    /// order, in Data Files — answered first and the retexture above it was never
    /// consulted.
    ///
    /// The rule the game follows, and now this: the first *mod* that supplies any member
    /// of the family wins, and within one mod `.dds` wins.
    pub fn resolve_family(&self, rel: &str, exts: &[&str]) -> Option<Loc> {
        let mut found = None;
        self.family_walk(rel, exts, &mut |_, loc| {
            found = Some(loc);
            true
        });
        found
    }

    /// `resolve_family`'s walk: every place that has a candidate, in the order it is
    /// asked, handed to `visit` until it answers true. [`Vfs::resolve_family`] stops at the
    /// first; [`Vfs::providers`] takes them all.
    fn family_walk(&self, rel: &str, exts: &[&str], visit: &mut dyn FnMut(Place, Loc) -> bool) {
        let key = norm(rel);
        let stem = match key.rfind('.') {
            Some(i) if !key[i..].contains('/') => &key[..i],
            _ => &key[..],
        };
        // The stated spelling stays a candidate, second: a mod that ships exactly what
        // the record asked for should not lose to its own `.dds` of something else.
        let mut cands: Vec<String> = Vec::with_capacity(exts.len() + 1);
        for e in exts.iter().take(1) {
            cands.push(format!("{}{}", stem, e));
        }
        if !cands.iter().any(|c| *c == key) {
            cands.push(key.clone());
        }
        for e in exts.iter().skip(1) {
            let c = format!("{}{}", stem, e);
            if !cands.contains(&c) {
                cands.push(c);
            }
        }
        for (ri, r) in self.roots.iter().enumerate() {
            for c in &cands {
                if let Some(p) = r.index.get(c) {
                    if visit(Place::Root(ri), Loc::Disk(p.clone())) {
                        return;
                    }
                }
            }
        }
        // A candidate in every archive before the next candidate, the last-loaded archive
        // first - `packed`'s order.
        for c in &cands {
            for (ai, a) in self.archives.iter().enumerate().rev() {
                if a.has(c) && visit(Place::Archive(ai), Loc::Packed { archive: ai, key: c.clone() }) {
                    return;
                }
            }
        }
    }

    /// A texture by the name a record or a NIF gave it, wherever it really lives.
    ///
    /// Records are inconsistent about the `Textures/` prefix as well as the extension,
    /// so both are tried — the prefixed spelling first, because that is where a texture
    /// belongs and an unprefixed hit at the top level is the unusual case.
    pub fn resolve_texture(&self, rel: &str) -> Option<Loc> {
        let n = norm(rel);
        if !n.starts_with("textures/") {
            if let Some(p) = self.resolve_family(&format!("textures/{}", n), Self::TEX_EXTS) {
                return Some(p);
            }
        }
        self.resolve_family(&n, Self::TEX_EXTS)
    }

    /// Meshes are referenced without their `Meshes/` prefix in ESP records.
    pub fn resolve_mesh(&self, rel: &str) -> Option<Loc> {
        let n = norm(rel);
        if !n.starts_with("meshes/") {
            if let Some(p) = self.resolve(&format!("meshes/{}", n)) {
                return Some(p);
            }
        }
        self.resolve(&n)
    }

    /// Every file under `prefix` carrying one of `exts`, as overlay-relative
    /// paths with the on-disk capitalisation kept, first root winning exactly as
    /// it does when reading. This is what the asset browsers list.
    pub fn list(&self, prefix: &str, exts: &[&str]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        /* The archives after the roots, spelled the way the archive spells them — so a
           packed mesh is in the picker beside the loose ones and picks like one. Walked
           backwards, because the last archive loaded is the one that would be read, and the
           picker should show that one's spelling rather than a shadowed copy's. */
        for a in self.archives.iter().rev() {
            for name in a.names() {
                let key = norm(name);
                if !key.starts_with(prefix) || !exts.iter().any(|e| key.ends_with(e)) {
                    continue;
                }
                if self.roots.iter().any(|r| r.index.contains_key(&key)) {
                    continue; // a loose copy is what the picker will read
                }
                if !seen.insert(key) {
                    continue;
                }
                out.push(name.replace('\\', "/"));
            }
        }
        for r in &self.roots {
            for (key, path) in &r.index {
                if !key.starts_with(prefix) || !exts.iter().any(|e| key.ends_with(e)) {
                    continue;
                }
                if !seen.insert(key.clone()) {
                    continue;
                }
                let shown = path
                    .strip_prefix(&r.dir)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|| key.clone());
                out.push(shown);
            }
        }
        out.sort_unstable();
        out
    }

    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        self.load(&self.resolve(rel)?)
    }

    /// Every plugin file the overlay holds, with where it is on disk.
    ///
    /// Round 18bl, for the chooser: Robin wants to see the plugins his install *has*, not
    /// only the ones it has switched on — "I know I have more .esp files available
    /// unticked". A load order file names the second set; this is the first.
    ///
    /// Two narrowings that [`Vfs::list`] does not make, and both matter here:
    ///
    /// * **The root only.** A plugin is loaded because it sits beside `Morrowind.esm`; one
    ///   in a subfolder is a spare copy somebody kept, and offering it as loadable would be
    ///   offering something the game would never read.
    /// * **Loose files only.** `list` merges the archives in, and a plugin packed inside a
    ///   BSA is not a plugin the game loads either.
    ///
    /// The name is spelled as the disk spells it, since that is what a person reads in the
    /// dialogue and what the order file will be compared against — matching stays
    /// case-insensitive everywhere, as it is for every other name in the tool.
    pub fn plugin_files(&self) -> Vec<(String, PathBuf)> {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<(String, PathBuf)> = Vec::new();
        for r in &self.roots {
            for (key, path) in &r.index {
                if key.contains('/') || !crate::paths::is_plugin_name(key) {
                    continue;
                }
                if !seen.insert(key.clone()) {
                    continue; // the higher-priority root already answered for this name
                }
                let shown = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| key.clone());
                out.push((shown, path.clone()));
            }
        }
        out.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
        out
    }
}
