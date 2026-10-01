//! The load order as a list somebody can choose from, read before anything is parsed.
//!
//! Round 18bi. Robin: "To make it easier for mod creators, I want to add options to choose
//! which of the available .esp in one's install should be loaded." The choosing has to
//! happen *before* the parse — his own load order is 562 plugins deep, Tamriel Rebuilt and
//! Cyrodiil and Skyrim Home of the Nords among them, and skipping that wait is most of the
//! point. So this answers "what is in the order, and what needs what" from plugin headers
//! alone, and the parse happens afterwards on whatever came back ticked.
//!
//! The dependency half is why the header is worth reading rather than just listing names:
//! a chooser that lets you untick `Tamriel_Data.esm` while `TR_Mainland.esm` is still
//! loaded has produced a load order the game would refuse, quietly, and a grass plugin
//! generated against it would be wrong in ways nothing would say out loud.

use std::path::{Path, PathBuf};

/// One plugin, as the chooser needs to show it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Entry {
    /// The name exactly as the load order spells it — that spelling is what the order
    /// file, the chooser and `open_install`'s filter all agree on.
    pub name: String,
    /// Its masters, lower-cased, in the order the header names them.
    pub masters: Vec<String>,
    /// A master, by its extension. Kept as a fact about the file rather than a rule about
    /// what may be unticked: Robin's design makes nothing un-untickable by fiat, it lets
    /// the dependencies decide.
    pub esm: bool,
    /// Named by the load order and not found in the overlay. Shown, because a load order
    /// that names a plugin you do not have is worth seeing, but there is nothing to load.
    pub missing: bool,
    /// Found, but its header would not read — an empty file, or something that is not a
    /// plugin under a plugin's name.
    pub unreadable: bool,
    /// Whether the install has this one switched on: named by the load order file, as
    /// against merely sitting in the overlay.
    ///
    /// Round 18bl. This is what the chooser preticks from, and what "Back to install"
    /// returns to — so it is the install's own state, not "everything there is".
    pub active: bool,
}

/// Whether a name ends in a master's extension.
pub fn is_master(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".esm") || n.ends_with(".omwgame")
}

/// Reads one plugin's header off disk with two small reads rather than loading the file.
///
/// The first read is the 16 bytes that state how long the header is; the second is exactly
/// that many. A 200 MB plugin costs the same as a 2 KB one.
pub fn head_of(path: &Path) -> Option<crate::esp::PluginHead> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut first = [0u8; 16];
    f.read_exact(&mut first).ok()?;
    let want = crate::esp::head_len(&first)?;
    /* A header claiming to be enormous is a damaged file, not an instruction to allocate
       what it asked for. The cap is far above any real header and far below anything that
       would hurt. */
    let want = want.min(1 << 20);
    let mut buf = vec![0u8; want];
    buf[..16].copy_from_slice(&first);
    f.seek(SeekFrom::Start(16)).ok()?;
    // A short read is fine: `plugin_head` reads what is there and stops.
    let mut got = 16;
    while got < want {
        match f.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(_) => break,
        }
    }
    buf.truncate(got);
    crate::esp::plugin_head(&buf)
}

/// The whole load order, resolved through an overlay and read header by header.
///
/// `order` is what the order file named, in its own order, which is the order the list
/// keeps: Robin asked for "the .esm and .esp files in the load order they were loaded in",
/// and it is also the only order in which the dependency answers mean anything.
pub fn list(
    names: &[String],
    active: &[String],
    resolve: impl Fn(&str) -> Option<PathBuf> + Sync,
) -> Vec<Entry> {
    let on: std::collections::HashSet<String> =
        active.iter().map(|n| n.to_ascii_lowercase()).collect();
    let paths: Vec<Option<PathBuf>> = names.iter().map(|n| resolve(n)).collect();
    let heads = crate::pool::par_map(&paths, |_, p| p.as_deref().and_then(head_of));
    names
        .iter()
        .zip(paths.iter())
        .zip(heads)
        .map(|((name, path), head)| Entry {
            name: name.clone(),
            masters: head.as_ref().map(|h| h.masters.clone()).unwrap_or_default(),
            esm: is_master(name),
            missing: path.is_none(),
            unreadable: path.is_some() && head.is_none(),
            active: on.contains(&name.to_ascii_lowercase()),
        })
        .collect()
}

/// Narrows a load order to the plugins named, keeping the order file's own order.
///
/// The names are compared without regard to case because they travel through a dialogue,
/// a JSON message and three different spellings of the same file on disk. A `None` here
/// means nobody chose, which is every plugin — not none of them, which is the failure mode
/// worth being deliberate about.
pub fn narrow(order: &[String], want: Option<&[String]>) -> Vec<String> {
    let Some(want) = want else { return order.to_vec() };
    let keep: std::collections::HashSet<String> =
        want.iter().map(|s| s.to_ascii_lowercase()).collect();
    order.iter().filter(|n| keep.contains(&n.to_ascii_lowercase())).cloned().collect()
}

/// Everything that has to load if `chosen` is to load: the chosen plugins and, through
/// however many steps it takes, every master they rest on.
///
/// This is the engine's own copy of the rule the chooser draws — the dash in a tick box —
/// and it is here so that a caller who never opened the dialogue (the command line, a
/// scripted run) cannot ask for a load order the game would refuse. Names not in `order`
/// are dropped: a master an install does not have cannot be pulled in by wanting it.
pub fn with_masters(all: &[Entry], chosen: &[String]) -> Vec<String> {
    use std::collections::{HashMap, HashSet};
    let by_name: HashMap<String, &Entry> =
        all.iter().map(|e| (e.name.to_ascii_lowercase(), e)).collect();
    let mut loaded: HashSet<String> = HashSet::new();
    let mut work: Vec<String> = Vec::new();
    for c in chosen {
        let k = c.to_ascii_lowercase();
        if by_name.contains_key(&k) && loaded.insert(k.clone()) {
            work.push(k);
        }
    }
    while let Some(n) = work.pop() {
        let Some(e) = by_name.get(&n) else { continue };
        for m in &e.masters {
            if by_name.contains_key(m) && loaded.insert(m.clone()) {
                work.push(m.clone());
            }
        }
    }
    // Back into load order, and back into the order file's own spelling.
    all.iter().filter(|e| loaded.contains(&e.name.to_ascii_lowercase())).map(|e| e.name.clone()).collect()
}

/// Every plugin an install *has*, in the order that install would show them.
///
/// Round 18bl. Robin: "I would like to see all the .esp files that I can see in MO2, even
/// the ones I don't have ticked usually. […] show all .esp files in the same order as they
/// appear in MO2. Same but equivalent should be there for Data Files manual install, and
/// OpenMW.cfg." A load order file names what is switched **on**; this is everything there
/// is, with the load order's own entries keeping their positions.
///
/// The three layouts answer differently because the three formats do:
///
/// * **MO2** writes `loadorder.txt` beside the profile's `Morrowind.ini` — every plugin it
///   knows about, ticked or not, in the order its plugin list shows them. On Robin's own
///   profile that is 678 lines against 562 in `[Game Files]`, and the active ones are an
///   exact subsequence of it, so the file can be taken as the display order outright.
/// * **Data Files** has no such file. Morrowind orders plugins by file date, masters
///   first, and the ini's order is the record of that — so an unticked plugin belongs where
///   its own date puts it. The ini is still the spine: its entries keep their positions
///   exactly, and the unticked are slotted between them. A hand-edited ini therefore still
///   loads in the order it states.
/// * **OpenMW** lists `content=` lines and nothing else: an addon the config does not name
///   has no position in it at all, so the unselected ones go after the content lines —
///   which is also where enabling one in the launcher would put it. Among themselves they
///   are ordered by **OpenMW's own algorithm**, ported in `openmw_rest`; Robin asked
///   whether that had been read rather than guessed at, and it had not been.
///
/// Whatever the layout: everything the load order names is in the answer even if the file
/// is gone (a load order naming what you do not have is worth seeing), everything on disk
/// is in it, and a `loadorder.txt` line that is neither is dropped as stale.
pub fn arrange(
    l: &crate::layout::Layout,
    order: &[String],
    on_disk: &[(String, PathBuf)],
    known: &[Entry],
) -> Vec<String> {
    use std::collections::{HashMap, HashSet};
    let active: HashSet<String> = order.iter().map(|n| n.to_ascii_lowercase()).collect();
    let have: HashMap<String, &PathBuf> =
        on_disk.iter().map(|(n, p)| (n.to_ascii_lowercase(), p)).collect();
    // Anything the install has that the load order does not name.
    let extras: Vec<String> = on_disk
        .iter()
        .filter(|(n, _)| !active.contains(&n.to_ascii_lowercase()))
        .map(|(n, _)| n.clone())
        .collect();

    // ---- MO2: its own file already says the whole order ---------------------------------
    if l.mo2_base.is_some() {
        if let Some(seq) = mo2_loadorder(&l.order_file) {
            let mut out: Vec<String> = Vec::with_capacity(seq.len());
            let mut placed: HashSet<String> = HashSet::new();
            for n in seq {
                let k = n.to_ascii_lowercase();
                // Stale: named by the file, but neither switched on nor on disk any more.
                if !active.contains(&k) && !have.contains_key(&k) {
                    continue;
                }
                if placed.insert(k.clone()) {
                    // The disk's spelling wins where there is one, the file's otherwise.
                    out.push(have.get(&k).and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or(n));
                }
            }
            // Anything the file has not caught up with yet: a mod enabled since it was
            // written, or a plugin dropped in by hand.
            for n in order.iter().chain(extras.iter()) {
                if placed.insert(n.to_ascii_lowercase()) {
                    out.push(n.clone());
                }
            }
            return out;
        }
    }

    // ---- OpenMW: content= order, then the rest in an order the game would accept --------
    if is_openmw(&l.order_file) {
        let mut out = order.to_vec();
        out.extend(openmw_rest(&extras, known));
        return out;
    }

    // ---- Data Files: the ini's spine, the rest slotted in by file date -------------------
    let key = |name: &str| -> (u8, std::time::SystemTime) {
        let stamp = have
            .get(&name.to_ascii_lowercase())
            .and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::UNIX_EPOCH);
        (if is_master(name) { 0 } else { 1 }, stamp)
    };
    let spine: Vec<(u8, std::time::SystemTime)> = order.iter().map(|n| key(n)).collect();
    // Where each extra lands: after every spine entry that sorts at or before it.
    let mut at: Vec<(usize, (u8, std::time::SystemTime), String)> = extras
        .into_iter()
        .map(|n| {
            let k = key(&n);
            (spine.iter().filter(|s| **s <= k).count(), k, n)
        })
        .collect();
    at.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.to_ascii_lowercase().cmp(&b.2.to_ascii_lowercase())));
    let mut out: Vec<String> = Vec::with_capacity(order.len() + at.len());
    let mut next = 0usize;
    for (i, n) in order.iter().enumerate() {
        while next < at.len() && at[next].0 <= i {
            out.push(at[next].2.clone());
            next += 1;
        }
        let _ = n;
        out.push(order[i].clone());
    }
    while next < at.len() {
        out.push(at[next].2.clone());
        next += 1;
    }
    out
}

/// The unselected addons, in the order OpenMW's own launcher would put them.
///
/// A port of `ContentSelectorModel::ContentModel::sortFiles`, from
/// `components/contentselector/model/contentmodel.cpp` — read rather than reasoned about,
/// because Robin asked whether it had been ("Did you check the OpenMW source code for how
/// their .esp files are sorted?") and it had not. Guessing had produced something close but
/// not the same, and "the order they would load into the game" is not a thing to approximate
/// when the code that decides it can simply be read.
///
/// What OpenMW does, and what this does with it:
///
/// * The list starts **alphabetical** — `addFiles` sets `QDir::Name` over `*.esp`, `*.esm`,
///   `*.omwgame`, `*.omwaddon`. (Those four are `PLUGIN_EXTS`, which is where the overlay
///   scan already gets its filter.)
/// * Then the **dependency sort**: walking from the end, each file is pulled up to just
///   before the **first** file above it that names it as a master, and that position is then
///   re-examined rather than stepped past. It is not a layered topological sort, and the
///   difference shows: over `Z`, `A_needs_Z`, `M` the alphabetical list is
///   `[A_needs_Z, M, Z]`, OpenMW answers `[Z, A_needs_Z, M]`, and sorting by depth answers
///   `[M, Z, A_needs_Z]`. OpenMW keeps a file where its name put it unless a dependency
///   actually drags it.
/// * And the part no header could ever have told us: **Bloodmoon is treated as depending on
///   Tribunal**, which its own masters do not say. OpenMW's comment calls it hallucinating
///   the dependency, and it is right to — the two expansions have to load in that order and
///   nothing in the files says so.
///
/// Two clauses of the original do not survive the port, and both are no-ops here rather
/// than omissions: built-in and non-user content are pinned above this region, and the
/// implicit "every addon depends on the game file" only fires for a file that is the game
/// file, which is in the `content=` spine and so loads before all of these regardless.
fn openmw_rest(extras: &[String], known: &[Entry]) -> Vec<String> {
    use std::collections::{HashMap, HashSet};
    // As `addFiles` leaves it.
    let mut files: Vec<String> = extras.to_vec();
    files.sort_by_key(|n| n.to_ascii_lowercase());

    let need: HashMap<String, Vec<String>> =
        known.iter().map(|e| (e.name.to_ascii_lowercase(), e.masters.clone())).collect();
    let here = |name: &str| files.iter().any(|n| n.eq_ignore_ascii_case(name));
    // `sortExpansions`: both have to be in the region for the clause to fire at all.
    let sort_expansions = here("Tribunal.esm") && here("Bloodmoon.esm");
    let depends_on = |addon: &str, file: &str| -> bool {
        if need
            .get(&addon.to_ascii_lowercase())
            .map(|ms| ms.iter().any(|m| m.eq_ignore_ascii_case(file)))
            .unwrap_or(false)
        {
            return true;
        }
        sort_expansions
            && file.eq_ignore_ascii_case("Tribunal.esm")
            && addon.eq_ignore_ascii_case("Bloodmoon.esm")
    };

    if files.len() < 2 {
        return files;
    }
    /* `moved` guards one position, not the whole walk: it stops a file that has just been
       pulled up from being pulled again while the cursor still sits where it was, and is
       cleared the moment the cursor steps back. That is the original's behaviour and the
       reason the loop terminates. */
    let mut moved: HashSet<String> = HashSet::new();
    let mut i = files.len() - 1;
    while i > 0 {
        let file = files[i].clone();
        let mut pulled = false;
        if !moved.contains(&file.to_ascii_lowercase()) {
            if let Some(j) = (0..i).find(|&j| depends_on(&files[j], &file)) {
                let f = files.remove(i);
                files.insert(j, f);
                moved.insert(file.to_ascii_lowercase());
                pulled = true; // and the cursor stays put, as the original's `continue` does
            }
        }
        if !pulled {
            i -= 1;
            moved.clear();
        }
    }
    /* And then made loadable, which the sort above does not guarantee.
     *
     * `moved` guards a position, so on some shapes the walk stops before it has converged:
     * over `A_Root` -> `B_Mid` -> `C_Top`, each naming only the next and named so that
     * alphabetical order runs against the chain, OpenMW answers
     * `[B_Mid, A_Root, C_Top]` — with `B_Mid` ahead of the master it rests on. Their
     * launcher shows that; issue #7085, "Support automatic load ordering in the launcher",
     * is the same weakness from the other end.
     *
     * Copying it exactly would be copying a bug into somewhere it costs more than a display
     * order: this list **is** the order a narrowed run parses in, so a plugin ahead of its
     * master here means a plugin generated against a load order the game would refuse.
     * So the walk above decides placement and this decides nothing except what it must —
     * seeded with the order it was handed, it takes the first file whose masters are all
     * already out, which leaves an order that is already loadable exactly as it was. */
    settle(files, &need, sort_expansions)
}

/// Makes an order loadable while moving as little of it as possible.
///
/// Seeded with the order it is given rather than with names or depth: a file only moves
/// because something it rests on has not been emitted yet, so wherever the caller's
/// ordering was already valid it survives untouched. A pair that names each other cannot be
/// satisfied at all — those are emitted in the order they arrived rather than dropped.
fn settle(
    order: Vec<String>,
    need: &std::collections::HashMap<String, Vec<String>>,
    sort_expansions: bool,
) -> Vec<String> {
    use std::collections::HashSet;
    let mine: HashSet<String> = order.iter().map(|n| n.to_ascii_lowercase()).collect();
    let masters = |n: &str| -> Vec<String> {
        let mut ms: Vec<String> = need
            .get(&n.to_ascii_lowercase())
            .map(|v| v.iter().filter(|m| mine.contains(*m)).cloned().collect())
            .unwrap_or_default();
        if sort_expansions && n.eq_ignore_ascii_case("Bloodmoon.esm") {
            ms.push("tribunal.esm".to_string());
        }
        ms
    };
    let mut left: Vec<String> = order;
    let mut out: Vec<String> = Vec::with_capacity(left.len());
    let mut done: HashSet<String> = HashSet::new();
    while !left.is_empty() {
        let next = left.iter().position(|n| masters(n).iter().all(|m| done.contains(m)));
        match next {
            Some(k) => {
                let n = left.remove(k);
                done.insert(n.to_ascii_lowercase());
                out.push(n);
            }
            // Nothing can go next, so something names something that names it back.
            None => {
                out.append(&mut left);
                break;
            }
        }
    }
    out
}

/// Whether the file a load order came from is an `openmw.cfg`.
fn is_openmw(order_file: &Path) -> bool {
    order_file
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase().ends_with(".cfg"))
        .unwrap_or(false)
}

/// MO2's own full plugin order, from `loadorder.txt` beside the profile's ini.
///
/// Comment lines and blanks are skipped; a UTF-8 BOM is stripped, because the file is
/// written by a Qt application on Windows and routinely carries one.
fn mo2_loadorder(order_file: &Path) -> Option<Vec<String>> {
    let dir = order_file.parent()?;
    let text = std::fs::read_to_string(dir.join("loadorder.txt")).ok()?;
    let out: Vec<String> = text
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| crate::world::is_plugin_name(l))
        .map(|l| l.to_string())
        .collect();
    (!out.is_empty()).then_some(out)
}
