//! The object inspector's "full help" - Wraithguard.
//!
//! Like the game's Toggle Full Help, and more: what the world keeps for drawing is not
//! everything a record says, so the inspector reads the rest on demand, from the plugin
//! files themselves, when an object is clicked:
//!
//! - [`ref_details`]: a placed reference's own fields in the plugin that last changed it -
//!   its owner (an NPC, or a faction and the rank it takes), the global that makes it
//!   owned, the lock, key and trap, a soul, charge, uses and count;
//! - [`record_fields`]: a base record's subrecords in the plugin that last defines it,
//!   and [`base_details`] the parts worth showing - its script, name, a container's or an
//!   actor's inventory, an actor's spells, race, class and faction;
//! - [`dialogue_of`]: across the whole load order, the topics an actor has lines of its
//!   own in (INFO `ONAM`), with how many and from which plugins, and the quests those
//!   lines touch - a journal their results write to (`Journal <id> <n>`) or their
//!   conditions test.
//!
//! Copyright (c) 2026 StaticNation, MIT (as the rest of viewcore).

use crate::esp::{cstring, f32le, i32le, u32le, Records, Subs, Tag};

/// Which cell a reference stands in: an interior by name, an exterior by grid.
#[derive(Clone, Debug)]
pub enum CellSel {
    Interior(String),
    Exterior(i32, i32),
}

/// A placed reference's own fields, as its last plugin writes them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefDetails {
    pub owner: String,
    pub owner_global: String,
    pub faction: String,
    /// The faction rank that may take it, when a faction owns it.
    pub rank: Option<i32>,
    pub soul: String,
    pub charge: Option<f32>,
    /// A weapon's or armour's health, or a light's time, or a tool's uses (INTV).
    pub uses: Option<i32>,
    pub count: Option<i32>,
    pub lock: Option<i32>,
    pub key: String,
    pub trap: String,
    pub blocked: bool,
}

fn cell_matches(body: &[u8], want: &CellSel) -> bool {
    let (mut name, mut flags, mut grid) = (String::new(), 0u32, (0i32, 0i32));
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => name = cstring(s.data),
            b"DATA" if s.data.len() >= 12 => {
                flags = u32le(s.data, 0);
                grid = (i32le(s.data, 4), i32le(s.data, 8));
            }
            // The header ends at the first reference.
            b"FRMR" | b"MVRF" => break,
            _ => {}
        }
    }
    match want {
        CellSel::Interior(n) => flags & 1 != 0 && name.eq_ignore_ascii_case(n),
        CellSel::Exterior(x, y) => flags & 1 == 0 && grid == (*x, *y),
    }
}

/// The reference `index` (FRMR's low 24 bits) with base id `id`, in `cell`, as the plugin
/// `buf` writes it; `None` when the plugin has no such reference there.
pub fn ref_details(buf: &[u8], cell: &CellSel, index: u32, id: &str) -> Option<RefDetails> {
    let mut recs = Records::new(buf);
    while let Some(rec) = recs.next_filtered(&|t: Tag| &t == b"CELL") {
        if !cell_matches(rec.body, cell) {
            continue;
        }
        let mut cur: Option<(u32, RefDetails, String)> = None;
        let mut hit: Option<RefDetails> = None;
        let done = |c: Option<(u32, RefDetails, String)>, hit: &mut Option<RefDetails>| {
            if let Some((ix, d, nm)) = c {
                if ix == index & 0x00ff_ffff && nm.eq_ignore_ascii_case(id) {
                    *hit = Some(d);
                }
            }
        };
        for s in Subs::new(rec.body) {
            let d = s.data;
            match &s.tag {
                b"FRMR" | b"MVRF" => {
                    done(cur.take(), &mut hit);
                    if &s.tag == b"FRMR" && d.len() >= 4 {
                        cur = Some((u32le(d, 0) & 0x00ff_ffff, RefDetails::default(), String::new()));
                    }
                }
                _ => {
                    let Some((_, r, nm)) = cur.as_mut() else { continue };
                    match &s.tag {
                        b"NAME" => *nm = cstring(d),
                        b"ANAM" => r.owner = cstring(d),
                        b"BNAM" => r.owner_global = cstring(d),
                        b"CNAM" => r.faction = cstring(d),
                        b"INDX" if d.len() >= 4 => r.rank = Some(i32le(d, 0)),
                        b"XSOL" => r.soul = cstring(d),
                        b"XCHG" if d.len() >= 4 => r.charge = Some(f32le(d, 0)),
                        b"INTV" if d.len() >= 4 => r.uses = Some(i32le(d, 0)),
                        b"NAM9" if d.len() >= 4 => r.count = Some(i32le(d, 0)),
                        b"FLTV" if d.len() >= 4 => r.lock = Some(i32le(d, 0)),
                        b"KNAM" => r.key = cstring(d),
                        b"TNAM" => r.trap = cstring(d),
                        b"UNAM" => r.blocked = true,
                        _ => {}
                    }
                }
            }
        }
        done(cur.take(), &mut hit);
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// Wraithguard: every reference's own fields in one cell of `buf`, in one pass - what the
/// Cell View's Ownership column reads: `(index & 0xffffff, id lower case) -> details`.
pub fn cell_details(buf: &[u8], cell: &CellSel) -> std::collections::HashMap<(u32, String), RefDetails> {
    let mut out = std::collections::HashMap::new();
    let mut recs = Records::new(buf);
    while let Some(rec) = recs.next_filtered(&|t: Tag| &t == b"CELL") {
        if !cell_matches(rec.body, cell) {
            continue;
        }
        let mut cur: Option<(u32, RefDetails, String)> = None;
        let flush = |c: Option<(u32, RefDetails, String)>, out: &mut std::collections::HashMap<(u32, String), RefDetails>| {
            if let Some((ix, d, nm)) = c {
                out.insert((ix, nm.to_ascii_lowercase()), d);
            }
        };
        for s in Subs::new(rec.body) {
            let d = s.data;
            match &s.tag {
                b"FRMR" | b"MVRF" => {
                    flush(cur.take(), &mut out);
                    if &s.tag == b"FRMR" && d.len() >= 4 {
                        cur = Some((u32le(d, 0) & 0x00ff_ffff, RefDetails::default(), String::new()));
                    }
                }
                _ => {
                    let Some((_, r, nm)) = cur.as_mut() else { continue };
                    match &s.tag {
                        b"NAME" => *nm = cstring(d),
                        b"ANAM" => r.owner = cstring(d),
                        b"BNAM" => r.owner_global = cstring(d),
                        b"CNAM" => r.faction = cstring(d),
                        b"INDX" if d.len() >= 4 => r.rank = Some(i32le(d, 0)),
                        _ => {}
                    }
                }
            }
        }
        flush(cur.take(), &mut out);
    }
    out
}

/// A record's id: NAME, or a script's SCHD name.
fn id_of(tag: Tag, body: &[u8]) -> Option<String> {
    crate::objects::record_id(tag, body)
}

/// The subrecords of the record `tag`/`id` in `buf` (the last one, if it is there twice).
pub fn record_fields(buf: &[u8], tag: Tag, id: &str) -> Option<Vec<(Tag, Vec<u8>)>> {
    let mut recs = Records::new(buf);
    let mut out = None;
    while let Some(rec) = recs.next_filtered(&|t: Tag| t == tag) {
        if id_of(rec.tag, rec.body).is_some_and(|n| n.eq_ignore_ascii_case(id)) {
            out = Some(Subs::new(rec.body).map(|s| (s.tag, s.data.to_vec())).collect());
        }
    }
    out
}

/// Every record in `buf` whose (lowercased) id is in `ids`: `id -> (tag, subrecords)`,
/// the last of each when it is there twice. How the inspector reads a container's items -
/// one pass over each plugin that defines some of them, not one per item.
pub fn records_many(buf: &[u8], ids: &std::collections::HashSet<String>) -> std::collections::HashMap<String, (Tag, Vec<(Tag, Vec<u8>)>)> {
    let mut out = std::collections::HashMap::new();
    let mut recs = Records::new(buf);
    while let Some(rec) = recs.next_filtered(&|t: Tag| !matches!(&t, b"CELL" | b"LAND" | b"PGRD" | b"INFO" | b"DIAL" | b"TES3")) {
        let Some(id) = id_of(rec.tag, rec.body) else { continue };
        let k = id.to_ascii_lowercase();
        if ids.contains(&k) {
            out.insert(k, (rec.tag, Subs::new(rec.body).map(|s| (s.tag, s.data.to_vec())).collect()));
        }
    }
    out
}

/// What the inspector shows of an item: its name, icon, mesh, and - for a leveled list -
/// its entries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemInfo {
    pub name: String,
    pub icon: String,
    pub model: String,
    pub list: Option<LeveledList>,
}

/// A leveled list's entries (level, id), the chance it gives nothing, and its flags.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeveledList {
    pub entries: Vec<(i32, String)>,
    pub chance_none: u8,
    /// "every level up to the PC's", "each item" (items: one roll per item in a stack).
    pub flags: Vec<&'static str>,
}

/// A leveled list's fields (LEVC's CNAM or LEVI's INAM entries, each followed by INTV).
pub fn leveled_list(fields: &[(Tag, Vec<u8>)]) -> LeveledList {
    let mut l = LeveledList::default();
    // A creature list's DATA has only "every level" (1); an item list's "each item" (1)
    // and "every level" (2).
    let creatures = fields.iter().any(|(t, _)| t == b"CNAM");
    for (t, d) in fields {
        match t {
            b"CNAM" | b"INAM" => l.entries.push((0, cstring(d))),
            b"INTV" if d.len() >= 2 => {
                if let Some(e) = l.entries.last_mut() {
                    e.0 = i16::from_le_bytes([d[0], d[1]]) as i32;
                }
            }
            b"NNAM" if !d.is_empty() => l.chance_none = d[0],
            b"DATA" if d.len() >= 4 => {
                let f = u32le(d, 0);
                if creatures {
                    if f & 0x1 != 0 {
                        l.flags.push("from every level up to the player's");
                    }
                } else {
                    if f & 0x1 != 0 {
                        l.flags.push("each item in a stack rolled apart");
                    }
                    if f & 0x2 != 0 {
                        l.flags.push("from every level up to the player's");
                    }
                }
            }
            _ => {}
        }
    }
    l
}

/// An item's name, icon and mesh (and a leveled list's entries).
pub fn item_info(tag: Tag, fields: &[(Tag, Vec<u8>)]) -> ItemInfo {
    let mut i = ItemInfo::default();
    for (t, d) in fields {
        match t {
            b"FNAM" => i.name = cstring(d),
            b"ITEX" => i.icon = cstring(d),
            b"MODL" => i.model = cstring(d),
            _ => {}
        }
    }
    if &tag == b"LEVI" || &tag == b"LEVC" {
        i.list = Some(leveled_list(fields));
    }
    i
}

/// The services an actor's AIDT offers, as words.
fn services(flags: u32) -> Vec<&'static str> {
    const S: [(u32, &str); 18] = [
        (0x1, "weapons"), (0x2, "armor"), (0x4, "clothing"), (0x8, "books"), (0x10, "ingredients"),
        (0x20, "picks"), (0x40, "probes"), (0x80, "lights"), (0x100, "apparatus"), (0x200, "repair items"),
        (0x400, "misc items"), (0x800, "spells"), (0x1000, "magic items"), (0x2000, "potions"),
        (0x4000, "training"), (0x8000, "spellmaking"), (0x10000, "enchanting"), (0x20000, "repair"),
    ];
    S.iter().filter(|(b, _)| flags & b != 0).map(|(_, n)| *n).collect()
}

/// What the inspector shows of a base record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BaseDetails {
    pub name: String,
    pub script: String,
    /// A container's or an actor's inventory: (count, item id). A negative count restocks.
    pub items: Vec<(i32, String)>,
    pub spells: Vec<String>,
    pub race: String,
    pub class: String,
    pub faction: String,
    pub female: bool,
    /// An actor's level (NPDT's first field), a container's capacity.
    pub level: Option<i32>,
    pub capacity: Option<f32>,
    /// FLAG's words: essential, respawns, organic...
    pub flags: Vec<&'static str>,
    /// An actor's AI: hello, fight, flee, alarm (AIDT).
    pub ai: Option<[u8; 4]>,
    /// What it sells and does (AIDT's services).
    pub services: Vec<&'static str>,
    /// Where it takes travellers: an interior's name, or the exterior grid "x, y".
    pub travel: Vec<String>,
    /// A leveled list's entries (the base record of a spawn point).
    pub list: Option<LeveledList>,
}

/// The parts of a record worth showing, by its kind.
pub fn base_details(tag: Tag, fields: &[(Tag, Vec<u8>)]) -> BaseDetails {
    let mut b = BaseDetails::default();
    for (t, d) in fields {
        match t {
            b"FNAM" if &tag != b"BODY" => b.name = cstring(d),
            b"SCRI" => b.script = cstring(d),
            b"NPCO" if d.len() >= 36 => b.items.push((i32le(d, 0), cstring(&d[4..36]))),
            b"NPCS" => b.spells.push(cstring(d)),
            b"RNAM" if &tag == b"NPC_" => b.race = cstring(d),
            b"CNAM" if &tag == b"NPC_" => b.class = cstring(d),
            b"ANAM" if &tag == b"NPC_" => b.faction = cstring(d),
            b"NPDT" if d.len() >= 2 && (&tag == b"NPC_" || &tag == b"CREA") => {
                // An NPC's level is a short; a creature's NPDT starts with its type, then level.
                b.level = Some(if &tag == b"NPC_" { i16::from_le_bytes([d[0], d[1]]) as i32 } else if d.len() >= 8 { i32le(d, 4) } else { 0 });
            }
            b"CNDT" if &tag == b"CONT" && d.len() >= 4 => b.capacity = Some(f32le(d, 0)),
            b"AIDT" if d.len() >= 12 && (&tag == b"NPC_" || &tag == b"CREA") => {
                b.ai = Some([d[0], d[2], d[3], d[4]]);
                b.services = services(u32le(d, 8));
            }
            // A travel destination: its position, then (for an interior) its cell's name.
            b"DODT" if d.len() >= 8 && (&tag == b"NPC_" || &tag == b"CREA") => {
                let (x, y) = ((f32le(d, 0) / 8192.0).floor() as i32, (f32le(d, 4) / 8192.0).floor() as i32);
                b.travel.push(format!("{x}, {y}"));
            }
            b"DNAM" if &tag == b"NPC_" || &tag == b"CREA" => {
                if let Some(last) = b.travel.last_mut() {
                    *last = cstring(d);
                }
            }
            b"FLAG" if d.len() >= 4 => {
                let f = u32le(d, 0);
                if &tag == b"NPC_" {
                    b.female = f & 0x1 != 0;
                    if f & 0x2 != 0 {
                        b.flags.push("essential");
                    }
                    if f & 0x4 != 0 {
                        b.flags.push("respawns");
                    }
                    if f & 0x10 != 0 {
                        b.flags.push("auto-calculated stats");
                    }
                } else if &tag == b"CREA" {
                    if f & 0x80 != 0 {
                        b.flags.push("essential");
                    }
                    if f & 0x100 != 0 {
                        b.flags.push("respawns");
                    }
                } else if &tag == b"CONT" {
                    if f & 0x1 != 0 {
                        b.flags.push("organic");
                    }
                    if f & 0x2 != 0 {
                        b.flags.push("respawns");
                    }
                }
            }
            _ => {}
        }
    }
    if &tag == b"LEVC" || &tag == b"LEVI" {
        b.list = Some(leveled_list(fields));
    }
    b
}

/// One topic an actor speaks in: its name, kind and how many of its own lines each
/// plugin gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Topic {
    pub name: String,
    /// DIAL's type: 0 topic, 1 voice, 2 greeting, 3 persuasion, 4 journal.
    pub kind: u8,
    pub lines: usize,
    /// Load-order indices of the plugins with lines for it.
    pub plugins: Vec<usize>,
}

/// An actor's dialogue across the load order (`bufs` in load order), and the journals
/// (quest ids) its lines write to or test.
pub fn dialogue_of(bufs: &[&[u8]], actor: &str) -> (Vec<Topic>, Vec<String>) {
    let mut topics: Vec<Topic> = Vec::new();
    let mut quests: Vec<String> = Vec::new();
    let add_quest = |q: &str, quests: &mut Vec<String>| {
        let q = q.trim().trim_matches('"').trim();
        if !q.is_empty() && !quests.iter().any(|x| x.eq_ignore_ascii_case(q)) {
            quests.push(q.to_string());
        }
    };
    for (pi, buf) in bufs.iter().enumerate() {
        let mut recs = Records::new(buf);
        let (mut topic, mut kind) = (String::new(), 0u8);
        while let Some(rec) = recs.next_filtered(&|t: Tag| &t == b"DIAL" || &t == b"INFO") {
            if &rec.tag == b"DIAL" {
                topic.clear();
                kind = 0;
                for s in Subs::new(rec.body) {
                    match &s.tag {
                        b"NAME" => topic = cstring(s.data),
                        b"DATA" if !s.data.is_empty() => kind = s.data[0],
                        _ => {}
                    }
                }
                continue;
            }
            let mut speaker = false;
            let mut result = String::new();
            let mut conds: Vec<String> = Vec::new();
            for s in Subs::new(rec.body) {
                match &s.tag {
                    b"ONAM" => speaker = cstring(s.data).eq_ignore_ascii_case(actor),
                    b"BNAM" => result = String::from_utf8_lossy(s.data).into_owned(),
                    // A condition: index, type ('4' a journal), function (2), comparison, name.
                    b"SCVR" if s.data.len() > 5 && s.data[1] == b'4' => conds.push(cstring(&s.data[5..])),
                    _ => {}
                }
            }
            if !speaker {
                continue;
            }
            let t = match topics.iter_mut().position(|t| t.name.eq_ignore_ascii_case(&topic) && t.kind == kind) {
                Some(i) => &mut topics[i],
                None => {
                    topics.push(Topic { name: topic.clone(), kind, ..Default::default() });
                    topics.last_mut().unwrap()
                }
            };
            t.lines += 1;
            if !t.plugins.contains(&pi) {
                t.plugins.push(pi);
            }
            for c in conds {
                add_quest(&c, &mut quests);
            }
            for line in result.lines() {
                let l = line.trim();
                let low = l.to_ascii_lowercase();
                let Some(rest) = low.strip_prefix("journal") else { continue };
                if !rest.starts_with([' ', ',', '\t', '"']) {
                    continue;
                }
                // The id, as written (quoted or not), before the stage number.
                let raw = &l[l.len() - rest.len()..].trim_start_matches([' ', ',', '\t']);
                let id = if let Some(q) = raw.strip_prefix('"') {
                    q.split('"').next().unwrap_or("")
                } else {
                    raw.split([' ', ',', '\t']).next().unwrap_or("")
                };
                add_quest(id, &mut quests);
            }
        }
    }
    topics.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase())));
    (topics, quests)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }
    fn z(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }
    fn rec(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn a_reference_s_owner_lock_and_trap() {
        let mut data = 1u32.to_le_bytes().to_vec(); // interior
        data.extend_from_slice(&[0u8; 8]);
        let mut body = sub(b"NAME", &z("Balmora, Caius Cosades' House"));
        body.extend(sub(b"DATA", &data));
        body.extend(sub(b"FRMR", &5u32.to_le_bytes()));
        body.extend(sub(b"NAME", &z("chest_small_01")));
        body.extend(sub(b"FRMR", &((1u32 << 24) | 7).to_le_bytes()));
        body.extend(sub(b"NAME", &z("chest_small_02")));
        body.extend(sub(b"ANAM", &z("caius cosades")));
        body.extend(sub(b"FLTV", &30i32.to_le_bytes()));
        body.extend(sub(b"KNAM", &z("key_caius")));
        body.extend(sub(b"TNAM", &z("trap_fire00")));
        let buf = rec(b"CELL", &body);
        let cell = CellSel::Interior("balmora, caius cosades' house".into());
        let d = ref_details(&buf, &cell, 7, "Chest_Small_02").unwrap();
        assert_eq!(d.owner, "caius cosades");
        assert_eq!((d.lock, d.key.as_str(), d.trap.as_str()), (Some(30), "key_caius", "trap_fire00"));
        assert_eq!(ref_details(&buf, &cell, 5, "chest_small_01"), Some(RefDetails::default()));
        assert!(ref_details(&buf, &CellSel::Exterior(0, 0), 7, "chest_small_02").is_none());
    }

    #[test]
    fn an_actor_s_topics_and_quests() {
        let mut dial = sub(b"NAME", &z("Background"));
        dial.extend(sub(b"DATA", &[0]));
        let mut info = sub(b"INAM", &z("1"));
        info.extend(sub(b"ONAM", &z("Caius Cosades")));
        let mut scvr = b"04JX0".to_vec();
        scvr.extend_from_slice(b"A1_1_FindSpymaster");
        info.extend(sub(b"SCVR", &scvr));
        info.extend(sub(b"BNAM", b"Journal \"A1_2_AntabolisInformant\" 10\r\nAddTopic \"x\""));
        let mut other = sub(b"INAM", &z("2"));
        other.extend(sub(b"ONAM", &z("someone else")));
        let mut buf = rec(b"DIAL", &dial);
        buf.extend(rec(b"INFO", &info));
        buf.extend(rec(b"INFO", &other));
        let (topics, quests) = dialogue_of(&[&buf], "caius cosades");
        assert_eq!(topics, vec![Topic { name: "Background".into(), kind: 0, lines: 1, plugins: vec![0] }]);
        assert_eq!(quests, vec!["A1_1_FindSpymaster".to_string(), "A1_2_AntabolisInformant".to_string()]);
    }

    #[test]
    fn a_container_s_contents_and_flags() {
        let mut npco = 3i32.to_le_bytes().to_vec();
        let mut id = b"ingred_bread_01".to_vec();
        id.resize(32, 0);
        npco.extend(id);
        let fields = vec![
            (*b"NAME", z("barrel_01")),
            (*b"FNAM", z("Barrel")),
            (*b"CNDT", 50f32.to_le_bytes().to_vec()),
            (*b"FLAG", 2u32.to_le_bytes().to_vec()),
            (*b"NPCO", npco),
        ];
        let b = base_details(*b"CONT", &fields);
        assert_eq!(b.name, "Barrel");
        assert_eq!(b.items, vec![(3, "ingred_bread_01".to_string())]);
        assert_eq!(b.capacity, Some(50.0));
        assert_eq!(b.flags, vec!["respawns"]);
    }
}
