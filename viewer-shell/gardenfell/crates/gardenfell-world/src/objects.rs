//! Scattering a reference by id — round 18av.
//!
//! Robin: "I want to be able to scatter a reference based on ID (which means an existing
//! thing from a master). Make it a picker that searches through all references from my
//! load order of the categories: Static, Activator, Creature, Leveled Creature. This is to
//! help with populating large landmasses, and I want to be able to populate with creatures
//! too."
//!
//! A card can name an object id instead of a mesh (`Slot::id`; Groundcover Generator's
//! `sID`). A rule with such a card places *references to that record* rather than to a
//! `GRS_` static of its own — and since a creature or an activator has to be loaded by
//! the game to mean anything, those references go into a **regular** plugin, apart from
//! the groundcover one, with the masters it needs. This module owns the four questions
//! that raises:
//!
//! - **which records exist** — every STAT, ACTI, CREA and LEVC across the load order,
//!   with the plugins that define each ([`ObjectDef`], indexed by [`World::load_with`]);
//! - **where a reference comes from** — a record an `.esm` defines is had through a
//!   master; one only an `.esp` defines cannot be (a plugin cannot master an `.esp`), so
//!   the record is **copied**, byte for byte and under its own id, into the plugin
//!   ([`plan`]);
//! - **what the copy drags along** — the script, the inventory, the spells, the leveled
//!   list's entries, an item's enchantment and body parts, a creature's sound generators:
//!   the dependency closure, copied too when asked ([`deps_of`], [`plan`]);
//! - **the masters** — `Morrowind.esm`, `Tribunal.esm` and `Bloodmoon.esm` always, when
//!   the load order has them (Robin), plus any other `.esm` that defines a record the
//!   plugin refers to.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::esp::Tag;
use crate::world::World;

// The record-level pieces live in `crate::records` (18ge); re-exported so every
// `objects::ObjKind`, `objects::record_id` and friend still resolves.
pub use crate::records::{
    deps_of, index_records, is_esm, raw_object, record_bytes, record_id, resolve_leveled, sound_gens_of,
    ObjKind, ObjectDef, PluginIndex, RawObject, ALWAYS_MASTERS, LEVELED_MARKER,
};

// The lift, tilt and model-of-a-slot helpers live in `crate::lift` (18ge): they read a
// rule's slots, which is the scatter's business and not a record's.


/// The records matching a search: id or name containing `q` (case-insensitively), of
/// the wanted kinds (all four when empty), by id, at most `limit`. An empty `q` lists
/// everything of those kinds — the picker's starting view.
pub fn search<'a>(world: &'a World, q: &str, kinds: &[ObjKind], limit: usize) -> Vec<&'a ObjectDef> {
    let ql = q.trim().to_ascii_lowercase();
    let mut out: Vec<&ObjectDef> = world
        .objects
        .values()
        .filter(|o| o.kind.is_some_and(|k| kinds.is_empty() || kinds.contains(&k)))
        .filter(|o| ql.is_empty() || o.id.to_ascii_lowercase().contains(&ql) || o.name.to_ascii_lowercase().contains(&ql))
        .collect();
    out.sort_by(|a, b| {
        // Ids that *start* with the text first, then the rest alphabetically.
        let sa = a.id.to_ascii_lowercase().starts_with(&ql);
        let sb = b.id.to_ascii_lowercase().starts_with(&ql);
        sb.cmp(&sa).then_with(|| a.id.to_ascii_lowercase().cmp(&b.id.to_ascii_lowercase()))
    });
    out.truncate(limit.max(1));
    out
}

/// Where a reference's record comes from, as the export dialogue says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Defined by these masters (load-order indices): the plugin names the last of them
    /// and needs no copy.
    Master(Vec<usize>),
    /// Defined only by these `.esp` files: one of them is copied from.
    Copy(Vec<usize>),
    /// The load order no longer defines it — a card naming a record that went away.
    Missing,
}

/// Which plugins define an id, split the way the export needs: masters first.
pub fn source_of(world: &World, id: &str) -> Source {
    let Some(o) = world.objects.get(&id.to_ascii_lowercase()) else { return Source::Missing };
    let esm: Vec<usize> = o.defined_in.iter().copied().filter(|&i| is_esm(&world.plugins[i])).collect();
    if !esm.is_empty() {
        return Source::Master(esm);
    }
    if o.defined_in.is_empty() {
        return Source::Missing;
    }
    Source::Copy(o.defined_in.clone())
}

/* ---- records, by id, inside one plugin file ---------------------------------------- */

/* ---- the plan ------------------------------------------------------------------------ */

/// One record the objects plugin carries a copy of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copied {
    pub id: String,
    pub tag: String,
    /// The plugin it was copied from, by load-order index.
    pub from: usize,
    /// Whether it was asked for, or came along as a dependency of one that was.
    pub dependency: bool,
    /// The picked ids this record is carried for (lowercased): itself, or every record it
    /// is a dependency of. A plugin that places none of them leaves the record out.
    ///
    /// Round 18bc (A5): a list rather than one id. Two cards naming two creatures from
    /// one `.esp` that share a script got the script copied once, tagged with the *first*
    /// creature — so in a split export a file placing only the second creature filtered
    /// the script out, and defined a creature whose `SCRI` named a script nothing
    /// provided. The record is still carried once, because two records with one id is the
    /// conflict this module exists to avoid; it is simply filed under both.
    pub roots: Vec<String>,
}

/// One record the plan carries — its bytes, verbatim, and the picked ids it belongs to.
#[derive(Clone, Debug)]
pub struct PlanRecord {
    pub bytes: Vec<u8>,
    /// See `Copied::roots`.
    pub roots: Vec<String>,
}

/// A dependency the plan could not satisfy: nothing in the load order defines it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmet {
    pub id: String,
    pub needed_by: String,
}

/// What the objects plugin is written with: its masters (name and byte size, in load
/// order), the raw records to carry, and the account of them.
///
/// Built once for a run and shared by every plugin it writes; each plugin takes the
/// records whose `root` it places (`for_roots`), so a region that places none of the
/// copied creatures redefines nothing. The masters are the run's, whole: the three
/// vanilla ones always, and Robin's rule for the rest is "any other that defines the
/// record" - a file naming a master it did not strictly need loads all the same.
#[derive(Clone, Debug, Default)]
pub struct ObjectsPlan {
    pub masters: Vec<(String, u64)>,
    pub records: Vec<PlanRecord>,
    pub copied: Vec<Copied>,
    pub unmet: Vec<Unmet>,
}

impl ObjectsPlan {
    /// The plan cut down to the picks in `roots` (lowercased ids): the records, the
    /// account of them and the unmet dependencies that belong to those.
    pub fn for_roots(&self, roots: &HashSet<String>) -> ObjectsPlan {
        ObjectsPlan {
            masters: self.masters.clone(),
            records: self.records.iter().filter(|r| r.roots.iter().any(|x| roots.contains(x))).cloned().collect(),
            copied: self.copied.iter().filter(|c| c.roots.iter().any(|x| roots.contains(x))).cloned().collect(),
            unmet: self
                .unmet
                .iter()
                .filter(|u| roots.contains(&if u.needed_by.is_empty() { u.id.to_ascii_lowercase() } else { u.needed_by.to_ascii_lowercase() }))
                .cloned()
                .collect(),
        }
    }
}

/// Where a dependency would come from, for the dialogue's preview and for the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepFrom {
    /// A master has it: nothing to copy (the master's index).
    Master(usize),
    /// Copied from this `.esp` (the index).
    Esp(usize),
    Missing,
}

/// The records `id` (in plugin `from`) depends on, transitively, with where each would
/// come from — what the dialogue lists under "Also copy what it depends on". The record's
/// own sound generators are included for a creature. `read` hands over a plugin's bytes
/// by index, so the caller decides how files are cached.
pub fn dependencies(
    world: &World,
    id: &str,
    from: usize,
    read: &mut dyn FnMut(usize) -> Option<std::sync::Arc<Vec<u8>>>,
) -> Vec<(String, String, DepFrom)> {
    let mut out: Vec<(String, String, DepFrom)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(id.to_ascii_lowercase());
    let mut queue: Vec<(String, usize)> = vec![(id.to_string(), from)];
    let mut indexes: HashMap<usize, PluginIndex<'static>> = HashMap::new();
    while let Some((cur, plugin)) = queue.pop() {
        let Some(buf) = read(plugin) else { continue };
        if !indexes.contains_key(&plugin) {
            indexes.insert(plugin, index_records(&buf).into_owned());
        }
        let Some(recs) = indexes.get(&plugin).and_then(|ix| ix.get(&cur)) else { continue };
        let mut wanted: Vec<String> = Vec::new();
        for (_, rec) in recs {
            for d in deps_of(rec) {
                if seen.insert(d.to_ascii_lowercase()) {
                    wanted.push(d);
                }
            }
        }
        for d in wanted {
            let (tag, where_) = locate(world, &d, plugin, read);
            if let DepFrom::Esp(p) = where_ {
                queue.push((d.clone(), p));
            }
            out.push((d, tag, where_));
        }
    }
    out
}

/// Where a record by that id is: a master that has it (any `.esm` in the load order),
/// else the `.esp` asked about, else the last `.esp` that defines it, else nowhere. The
/// tag is the record's, for the report; empty when unknown.
fn locate(
    world: &World,
    id: &str,
    prefer: usize,
    read: &mut dyn FnMut(usize) -> Option<std::sync::Arc<Vec<u8>>>,
) -> (String, DepFrom) {
    let key = id.to_ascii_lowercase();
    if let Some(defs) = world.record_where.get(&key) {
        if let Some((p, t)) = defs.iter().filter(|(p, _)| is_esm(&world.plugins[*p])).last() {
            return (tag_str(*t), DepFrom::Master(*p));
        }
        if let Some((p, t)) = defs.iter().find(|(p, _)| *p == prefer) {
            return (tag_str(*t), DepFrom::Esp(*p));
        }
        if let Some((p, t)) = defs.last() {
            return (tag_str(*t), DepFrom::Esp(*p));
        }
    }
    // Not in the index (a record kind it does not cover): look in the preferred file.
    if let Some(buf) = read(prefer) {
        if let Some(recs) = index_records(&buf).get(&key) {
            if let Some((t, _)) = recs.first() {
                return (tag_str(*t), DepFrom::Esp(prefer));
            }
        }
    }
    (String::new(), DepFrom::Missing)
}

fn tag_str(t: Tag) -> String {
    String::from_utf8_lossy(&t).to_string()
}

/* ---- which version of each record, and what comes along (round 18fy) --------------

   Robin: "If a dependency ONLY exists in the masters (that is, no .esp file has ever
   touched it in the entire load order), then we don't have to write it out in the list
   [...] For all dependencies that exist in both masters and other .esp files, let me tick
   them individually [...] Default should be off (as in default is the .esp just relies on
   the record from the master and don't copy the edited dependency). For all the
   dependencies that only have a dependency copied from another .esp file, let me tick
   them individually on and off as well, default ON. [...] choose which .esp file I want
   to copy the dependency record from." And, settled in the talk after it: the master row
   is a dropdown where the master means "don't copy"; the same three kinds apply to the
   objects picked on cards, not only to what they depend on; no `.esp` is ever made a
   master ("something that can't be opened in the Construction Set is also a big red
   flag").

   The reason for the granularity, in his words: "to support both users just making a mod
   for themselves (to just go for the master, and not copy dependencies is the way to go),
   but also mod creators exporting mods to be used by others, in which case, they might
   not want to always need other mods as dependencies to run the mod they want to share."

   So every record the export touches is one of four [`Class`]es, and one [`resolve`]
   answers both the dialogue and the plan - the dialogue lists what it will do and the
   plan does exactly that, because they are the same computation. */

/// How a record can be had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Only masters define it: the master is named and nothing is asked.
    MasterOnly,
    /// A master defines it and at least one `.esp` edits it: the master by default (no
    /// copy), or an `.esp`'s version copied. Copying it overrides that record for the
    /// whole game, which is why the master is the default.
    Choose,
    /// Only `.esp` files define it: copied from one of them, or (a dependency, unticked)
    /// not copied at all - the export then relies on that `.esp` being loaded.
    Copy,
    /// Nothing in the load order defines it.
    Missing,
}

/// One record the export places or depends on, and how it is had.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    /// The record's tag as the last plugin to define it spells it.
    pub tag: String,
    pub class: Class,
    /// The masters and the `.esp` files that define it, each in load order.
    pub masters: Vec<usize>,
    pub esps: Vec<usize>,
    /// Where it is had from: a master (named, not copied), an `.esp` (copied), or `None`
    /// - missing, or a `Copy` dependency the person chose not to copy.
    pub from: Option<usize>,
    /// The records that refer to it (their ids, as written). Empty for a pick.
    pub needed_by: Vec<String>,
    /// The picks it is carried for (lowercased) - see `Copied::roots`.
    pub roots: Vec<String>,
}

impl Item {
    /// Whether this record is copied into the plugin.
    pub fn copied(&self) -> bool {
        self.from.is_some_and(|f| self.esps.contains(&f))
    }
}

/// Everything the objects plugin places and depends on, decided.
#[derive(Clone, Debug, Default)]
pub struct Resolution {
    pub picks: Vec<Item>,
    /// Every dependency met on the way, `MasterOnly` ones included (the masters' tooltips
    /// list them); in the order they were found.
    pub deps: Vec<Item>,
    /// The masters the plugin names, in load order, with the ids each provides.
    pub masters: Vec<(usize, Vec<String>)>,
    pub unmet: Vec<Unmet>,
}

/// The person's choices, by lowercased id: the plugin to have it from (a master means
/// "do not copy"), or `None` for a `Copy` dependency left unticked. An absent id takes
/// the default, and so does a choice that no longer names a plugin that defines it.
pub type Choices = HashMap<String, Option<usize>>;

/// Which plugins define `id`, masters and `.esp` files apart, in load order, and the tag.
/// A pick is asked of the object index, which drops a definition a later plugin deletes;
/// anything else of the id index, which knows every record with a name.
pub fn sources(world: &World, id: &str) -> (Vec<usize>, Vec<usize>, String) {
    let key = id.to_ascii_lowercase();
    let mut defs: Vec<usize> = Vec::new();
    let mut tag = String::new();
    if let Some(o) = world.objects.get(&key) {
        defs = o.defined_in.clone();
    }
    if let Some(w) = world.record_where.get(&key) {
        if defs.is_empty() {
            defs = w.iter().map(|(p, _)| *p).collect();
        }
        if let Some((_, t)) = w.last() {
            tag = tag_str(*t);
        }
    }
    defs.sort_unstable();
    defs.dedup();
    let (m, e): (Vec<usize>, Vec<usize>) = defs.into_iter().partition(|&p| world.plugins.get(p).is_some_and(|n| is_esm(n)));
    (m, e, tag)
}

fn classify(masters: &[usize], esps: &[usize]) -> Class {
    match (masters.is_empty(), esps.is_empty()) {
        (false, true) => Class::MasterOnly,
        (false, false) => Class::Choose,
        (true, false) => Class::Copy,
        (true, true) => Class::Missing,
    }
}

/// Where a record is had from, given the class, the person's choice and whether it is a
/// pick (a pick is placed, so it is never left out).
fn decide(class: Class, masters: &[usize], esps: &[usize], choice: Option<&Option<usize>>, pick: bool) -> Option<usize> {
    let valid = |p: usize| masters.contains(&p) || esps.contains(&p);
    match class {
        Class::Missing => None,
        Class::MasterOnly => masters.last().copied(),
        Class::Choose => match choice {
            Some(Some(p)) if valid(*p) => Some(*p),
            _ => masters.last().copied(),
        },
        Class::Copy => match choice {
            Some(Some(p)) if valid(*p) => Some(*p),
            Some(None) if !pick => None,
            _ => esps.last().copied(),
        },
    }
}

/// Decides every record the export places and, with `copy_deps`, everything they depend
/// on. A record's dependencies are followed when the record is in the plugin - placed (a
/// pick) or copied - and read from the version chosen for it, since another plugin's
/// version may name other things. A dependency had from a master is not followed: none
/// of it is in the file, so what it refers to is the master's business.
pub fn resolve(
    world: &World,
    picks: &[String],
    choices: &Choices,
    copy_deps: bool,
    read: &mut dyn FnMut(usize) -> Option<std::sync::Arc<Vec<u8>>>,
) -> Resolution {
    let mut out = Resolution::default();
    let mut masters: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (i, name) in world.plugins.iter().enumerate() {
        if ALWAYS_MASTERS.contains(&name.to_ascii_lowercase().as_str()) {
            masters.insert(i, Vec::new());
        }
    }
    let provide = |masters: &mut BTreeMap<usize, Vec<String>>, of: &[usize], id: &str| {
        for &m in of {
            let v = masters.entry(m).or_default();
            if !v.iter().any(|x| x.eq_ignore_ascii_case(id)) {
                v.push(id.to_string());
            }
        }
    };
    // Where each id sits: (is a pick, index into picks or deps).
    let mut at: HashMap<String, (bool, usize)> = HashMap::new();
    // Edges parent -> children, lowercased, through records that are in the plugin.
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    let mut expand: Vec<(String, usize)> = Vec::new();
    for id in picks {
        let key = id.to_ascii_lowercase();
        if at.contains_key(&key) {
            continue;
        }
        let (m, e, tag) = sources(world, id);
        let class = classify(&m, &e);
        let from = decide(class, &m, &e, choices.get(&key), true);
        if class == Class::Missing {
            out.unmet.push(Unmet { id: id.clone(), needed_by: String::new() });
        }
        if let Some(f) = from {
            if m.contains(&f) {
                provide(&mut masters, &m, id);
            }
            if copy_deps {
                expand.push((id.clone(), f));
            }
        }
        at.insert(key.clone(), (true, out.picks.len()));
        out.picks.push(Item { id: id.clone(), tag, class, masters: m, esps: e, from, needed_by: Vec::new(), roots: vec![key] });
    }
    let mut indexes: HashMap<usize, PluginIndex<'static>> = HashMap::new();
    while let Some((cur, plugin)) = expand.pop() {
        let Some(buf) = read(plugin) else { continue };
        if !indexes.contains_key(&plugin) {
            indexes.insert(plugin, index_records(&buf).into_owned());
        }
        let Some(recs) = indexes.get(&plugin).and_then(|ix| ix.get(&cur)) else { continue };
        let mut found: Vec<String> = Vec::new();
        for (_, rec) in recs {
            for d in deps_of(rec) {
                if !found.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
                    found.push(d);
                }
            }
        }
        let ckey = cur.to_ascii_lowercase();
        for d in found {
            let dkey = d.to_ascii_lowercase();
            if dkey == ckey {
                continue;
            }
            edges.entry(ckey.clone()).or_default().push(dkey.clone());
            if let Some(&(pick, i)) = at.get(&dkey) {
                let it = if pick { &mut out.picks[i] } else { &mut out.deps[i] };
                if !pick && !it.needed_by.iter().any(|x| x.eq_ignore_ascii_case(&cur)) {
                    it.needed_by.push(cur.clone());
                }
                continue;
            }
            let (m, e, tag) = sources(world, &d);
            let class = classify(&m, &e);
            let from = decide(class, &m, &e, choices.get(&dkey), false);
            match class {
                Class::Missing => out.unmet.push(Unmet { id: d.clone(), needed_by: cur.clone() }),
                _ => {
                    if let Some(f) = from {
                        if m.contains(&f) {
                            provide(&mut masters, &m, &d);
                        } else {
                            // Copied: what it refers to comes along in turn.
                            expand.push((d.clone(), f));
                        }
                    }
                }
            }
            at.insert(dkey, (false, out.deps.len()));
            out.deps.push(Item { id: d, tag, class, masters: m, esps: e, from, needed_by: vec![cur.clone()], roots: Vec::new() });
        }
    }
    /* Which picks each dependency is carried for: everything reachable from a pick
       through records that are in the plugin. Worked out once the graph is whole, so a
       dependency two picks share is filed under both (round 18bc's A5). */
    for pi in 0..out.picks.len() {
        let root = out.picks[pi].id.to_ascii_lowercase();
        let mut stack = vec![root.clone()];
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(k) = stack.pop() {
            if !seen.insert(k.clone()) {
                continue;
            }
            let Some(ch) = edges.get(&k) else { continue };
            for c in ch.clone() {
                if let Some(&(false, i)) = at.get(&c) {
                    let it = &mut out.deps[i];
                    if !it.roots.contains(&root) {
                        it.roots.push(root.clone());
                    }
                    if it.copied() {
                        stack.push(c);
                    }
                }
            }
        }
    }
    out.masters = masters.into_iter().collect();
    out
}

/// The plan for an objects plugin that refers to `picks` — each an id and the plugin to
/// have it from when there is a choice. `copy_deps` follows the dependencies too, each
/// at its default. See [`plan_with`] for a plan with the person's choices for those.
pub fn plan(
    world: &World,
    picks: &[(String, Option<usize>)],
    copy_deps: bool,
    read: &mut dyn FnMut(usize) -> Option<std::sync::Arc<Vec<u8>>>,
) -> ObjectsPlan {
    let mut choices: Choices = HashMap::new();
    for (id, c) in picks {
        if let Some(p) = c {
            choices.insert(id.to_ascii_lowercase(), Some(*p));
        }
    }
    let ids: Vec<String> = picks.iter().map(|(i, _)| i.clone()).collect();
    plan_with(world, &ids, &choices, copy_deps, read)
}

/// The plan, from [`resolve`]: the masters it names, and a verbatim copy of every record
/// it decided to copy, filed under the picks that need it.
pub fn plan_with(
    world: &World,
    picks: &[String],
    choices: &Choices,
    copy_deps: bool,
    read: &mut dyn FnMut(usize) -> Option<std::sync::Arc<Vec<u8>>>,
) -> ObjectsPlan {
    let r = resolve(world, picks, choices, copy_deps, read);
    let mut out = ObjectsPlan::default();
    let mut indexes: HashMap<usize, PluginIndex<'static>> = HashMap::new();
    let items = r.picks.iter().map(|x| (x, false)).chain(r.deps.iter().map(|x| (x, true)));
    for (it, dependency) in items {
        if !it.copied() {
            continue;
        }
        let from = it.from.unwrap();
        let roots = if dependency { it.roots.clone() } else { vec![it.id.to_ascii_lowercase()] };
        let Some(first) = roots.first() else { continue };
        if let Some(at) = copy_record(&mut out, &it.id, from, dependency, first, &mut indexes, read) {
            for root in roots.iter().skip(1) {
                note_root(&mut out, &at, root);
            }
        }
    }
    out.unmet = r.unmet;
    for (i, _) in r.masters {
        let name = world.plugins[i].clone();
        let size = world.plugin_paths.get(i).and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
        out.masters.push((name, size));
    }
    out
}

/// Files one more picked id against records the plan already carries — round 18bc (A5).
fn note_root(out: &mut ObjectsPlan, at: &(Vec<usize>, Vec<usize>), root: &str) {
    for &i in &at.0 {
        if let Some(r) = out.records.get_mut(i) {
            if !r.roots.iter().any(|x| x == root) {
                r.roots.push(root.to_string());
            }
        }
    }
    for &i in &at.1 {
        if let Some(c) = out.copied.get_mut(i) {
            if !c.roots.iter().any(|x| x == root) {
                c.roots.push(root.to_string());
            }
        }
    }
}

/// Copies every record called `id` in plugin `from` into the plan — with a creature's
/// sound generators — and says where its entries landed, or `None` when nothing was
/// found. The indices are what lets a later pick be filed against the same records.
///
/// Round 18gh: the copy is `Record::write` of the indexed raw record, which gives back the
/// very bytes the file held (the raw layer is byte-exact; `tests/round18gg.rs` says so for
/// every plugin Robin has), so a record this program does not understand still travels
/// whole.
fn copy_record(
    out: &mut ObjectsPlan,
    id: &str,
    from: usize,
    dependency: bool,
    root: &str,
    indexes: &mut HashMap<usize, PluginIndex<'static>>,
    read: &mut dyn FnMut(usize) -> Option<std::sync::Arc<Vec<u8>>>,
) -> Option<(Vec<usize>, Vec<usize>)> {
    if !indexes.contains_key(&from) {
        let buf = read(from)?;
        indexes.insert(from, index_records(&buf).into_owned());
    }
    let ix = indexes.get(&from)?;
    let recs = ix.get(id)?;
    let mut creature = false;
    let mut at: (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    for (tag, rec) in recs {
        at.0.push(out.records.len());
        out.records.push(PlanRecord { bytes: record_bytes(rec), roots: vec![root.to_string()] });
        at.1.push(out.copied.len());
        out.copied.push(Copied {
            id: id.to_string(),
            tag: tag_str(tag),
            from,
            dependency,
            roots: vec![root.to_string()],
        });
        creature |= &tag == b"CREA";
    }
    if creature {
        for rec in sound_gens_of(ix, id) {
            at.0.push(out.records.len());
            out.records.push(PlanRecord { bytes: record_bytes(rec), roots: vec![root.to_string()] });
        }
    }
    Some(at)
}

/// Reads plugin files by load-order index, once each, for `plan` and `dependencies`.
pub fn file_reader(world: &World) -> impl FnMut(usize) -> Option<std::sync::Arc<Vec<u8>>> + '_ {
    let mut cache: HashMap<usize, std::sync::Arc<Vec<u8>>> = HashMap::new();
    move |i: usize| {
        if let Some(b) = cache.get(&i) {
            return Some(b.clone());
        }
        let p: &Path = world.plugin_paths.get(i)?;
        let b = std::sync::Arc::new(std::fs::read(p).ok()?);
        cache.insert(i, b.clone());
        Some(b)
    }
}
