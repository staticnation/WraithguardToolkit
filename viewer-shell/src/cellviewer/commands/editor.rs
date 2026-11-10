// The Editor: the Object Window's lists and the cell's references from the loaded world, the
// script and Lua charts, and the Editor's links answered by `editcore` (`editor_open`, `editor_link`).
// Part of the command layer (`../commands.rs`, which registers every command and says why
// each is `#[tauri::command(async)]`); split out by page, 2026-11-09.

use super::*;

/* ---- the Editor mode (Wraithguard, 50_wg_editor.js) ------------------------------------

   The Construction Set's Object Window and Cell View, from the world the viewer already
   loaded: what records each tag has and who defines them, and what references a cell
   holds without drawing it. A record's contents and every edit are Wraithguard's (the
   page asks it over the `links` the launch handed over); this only lists. */

/// Every reference in the world by base id (lower case) -> how many are placed. Counted
/// once per world (`rev`), the cells split across the pool, and kept: every Object Window
/// tab reads it, and it holds every reference the load order places, grass included.
pub(super) fn placed_counts(w: &World, rev: u64) -> std::sync::Arc<HashMap<String, u32>> {
    type Kept = (u64, std::sync::Arc<HashMap<String, u32>>);
    static KEPT: std::sync::Mutex<Option<Kept>> = std::sync::Mutex::new(None);
    if let Some((r, n)) = KEPT.lock().ok().and_then(|k| k.clone()) {
        if r == rev {
            return n;
        }
    }
    let cells: Vec<_> = w.cells.values().map(|c| &c.refs).collect();
    let rooms: Vec<_> = w.interiors.values().map(|c| &c.refs).collect();
    let mut parts = viewcore::pool::par_map(&cells, |_, refs| {
        let mut n: HashMap<String, u32> = HashMap::new();
        for r in refs.values().filter(|r| !r.deleted) {
            *n.entry(r.id.to_ascii_lowercase()).or_default() += 1;
        }
        n
    });
    parts.extend(viewcore::pool::par_map(&rooms, |_, refs| {
        let mut n: HashMap<String, u32> = HashMap::new();
        for r in refs.values().filter(|r| !r.deleted) {
            *n.entry(r.id.to_ascii_lowercase()).or_default() += 1;
        }
        n
    }));
    let mut n: HashMap<String, u32> = HashMap::new();
    for p in parts {
        for (k, v) in p {
            *n.entry(k).or_default() += v;
        }
    }
    let n = std::sync::Arc::new(n);
    if let Ok(mut k) = KEPT.lock() {
        *k = Some((rev, n.clone()));
    }
    n
}

/// The Object Window's tabs: `[[tag, count]...]`, every tag the load order has records of.
#[tauri::command(async)]
pub(super) fn editor_tags(state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let rows: Vec<String> = tag_counts(&w)
        .iter()
        .map(|(t, c)| {
            let mut s = String::from("[\"");
            viewcore::json::escape_into(t, &mut s);
            s.push_str(&format!("\",{c}]"));
            s
        })
        .collect();
    Ok(format!("[{}]", rows.join(",")))
}

/// Wraithguard: `editor_tags` as bytes (`WGTB`): one table, `tags` (`su`: tag, count).
#[tauri::command(async)]
pub(super) fn editor_tags_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    Ok(tauri::ipc::Response::new(pack_tags(&tag_counts(&w))))
}

pub(super) fn pack_tags(tags: &[(String, u64)]) -> Vec<u8> {
    let mut t = Tables::new();
    t.table("tags", "su", tags.len());
    for (tag, c) in tags {
        t.s(tag).u(*c);
    }
    t.done()
}

/// Every tag the load order has records of, with how many, in tag order.
pub(super) fn tag_counts(w: &viewcore::world::World) -> Vec<(String, u64)> {
    // Wraithguard: every live record in its own type's tab - an id defined as a weapon
    // and a misc item is in both, as OpenMW keeps both (`ordercore::records`).
    let mut n: HashMap<viewcore::esp::Tag, u64> = HashMap::new();
    for (tag, _id, _v) in w.records.iter() {
        *n.entry(*tag).or_default() += 1;
    }
    let mut tags: Vec<(viewcore::esp::Tag, u64)> = n.into_iter().collect();
    tags.sort();
    tags.into_iter().map(|(t, c)| (tag_text(&t), c)).collect()
}

/// One Object Window row: the id as shown, name, model, the plugins defining it (load-order
/// indices, the last winning) and how many are placed.
pub(super) struct ObjRow {
    pub(super) key: String,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) model: String,
    pub(super) plugins: Vec<usize>,
    pub(super) placed: u32,
}

pub(super) fn tag_of(tag: &str) -> viewcore::esp::Tag {
    let mut t = [b'_'; 4];
    for (i, b) in tag.bytes().take(4).enumerate() {
        t[i] = b.to_ascii_uppercase();
    }
    t
}

/// The Object Window's rows for one tag, sorted by id: straight from the record index the
/// world was loaded with (`ordercore::records`) - nothing the scene has drawn. The id is as
/// the record spells it where the world kept it (objects), else as the winning plugin does.
pub(super) fn object_rows(w: &World, rev: u64, want: viewcore::esp::Tag) -> Vec<ObjRow> {
    let placed = placed_counts(w, rev);
    let mut rows: Vec<ObjRow> = Vec::new();
    // Wraithguard: this type's live records (`ordercore::records`), whatever else the id is.
    for (tag, id, v) in w.records.iter() {
        if *tag != want {
            continue;
        }
        // The world's object, model and name are the placed type's: another type's row
        // shows its id alone.
        let is_placed = w.records.is_placed(tag, id);
        let obj = w.objects.get(id).filter(|_| is_placed);
        let shown = obj.map(|o| o.id.as_str()).filter(|s| !s.is_empty()).unwrap_or(&v.id);
        // Lights, statics and the like keep their mesh in `models` rather than `objects`.
        let model = obj
            .map(|o| o.model.as_str())
            .filter(|m| !m.is_empty())
            .or_else(|| w.models.get(id).filter(|_| is_placed).map(String::as_str));
        // Wraithguard: a land texture has no mesh; its texture file stands in the column
        // (the Editor's preview shows the texture itself).
        let model = model.or_else(|| if want == *b"LTEX" { w.ltex_file.get(id).map(String::as_str) } else { None });
        // A body part is never placed, so it has no object or model above; its mesh is in
        // the NPC tables (the parts an NPC is assembled from), and the preview needs it.
        let model = model.or_else(|| {
            if want == *b"BODY" {
                w.npc.bodies.get(&id.to_ascii_lowercase()).map(|b| b.model.as_str()).filter(|m| !m.is_empty())
            } else {
                None
            }
        });
        rows.push(ObjRow {
            key: id.to_string(),
            id: shown.to_string(),
            name: obj.map(|o| o.name.clone()).unwrap_or_default(),
            model: model.unwrap_or("").to_string(),
            plugins: v.plugins.clone(),
            // The references placed are of the type the id is placed as.
            placed: if is_placed { placed.get(id).copied().unwrap_or(0) } else { 0 },
        });
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}

pub(super) fn world_and_rev(state: &State) -> Result<(std::sync::Arc<World>, u64), String> {
    let app = state.app();
    let w = app.world.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    Ok((w, app.world_rev))
}

/// The Object Window's rows for one tag as JSON: `{plugins:[names], rows:[[id, name,
/// model, [plugin ix...], placed]...]}` - for the Editor's other lists (dialogue, place,
/// record), and the Object Window's fallback when `editor_rows` fails.
#[tauri::command(async)]
pub(super) fn editor_records(tag: String, state: State) -> Result<String, String> {
    let (w, rev) = world_and_rev(&state)?;
    let rows: Vec<String> = object_rows(&w, rev, tag_of(&tag))
        .into_iter()
        .map(|r| {
            let mut row = String::from("[\"");
            viewcore::json::escape_into(&r.id, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.name, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.model, &mut row);
            let ixs: Vec<String> = r.plugins.iter().map(usize::to_string).collect();
            row.push_str(&format!("\",[{}],{}]", ixs.join(","), r.placed));
            row
        })
        .collect();
    let mut o = J::obj();
    o.raw("plugins", &string_array(&w.plugins));
    o.raw("rows", &format!("[{}]", rows.join(",")));
    Ok(o.done())
}

/// Wraithguard: the Object Window's rows for one tag, as bytes (no JSON to go wrong):
/// `"WGOR"`, u32 version 1, u32 plugin count, each plugin a string; u32 row count, each row
/// id, name, model (strings), u32 plugin count, that many u32 indices, u32 placed. A string
/// is u32 byte length + UTF-8. All little-endian.
#[tauri::command(async)]
pub(super) fn editor_rows(tag: String, state: State) -> Result<tauri::ipc::Response, String> {
    let (w, rev) = world_and_rev(&state)?;
    Ok(tauri::ipc::Response::new(pack_rows(&w.plugins, &object_rows(&w, rev, tag_of(&tag)))))
}

pub(super) fn pack_rows(plugins: &[String], rows: &[ObjRow]) -> Vec<u8> {
    fn u32_(o: &mut Vec<u8>, v: usize) {
        o.extend_from_slice(&(v.min(u32::MAX as usize) as u32).to_le_bytes());
    }
    fn str_(o: &mut Vec<u8>, s: &str) {
        u32_(o, s.len());
        o.extend_from_slice(s.as_bytes());
    }
    let mut o = b"WGOR".to_vec();
    u32_(&mut o, 1);
    u32_(&mut o, plugins.len());
    for p in plugins {
        str_(&mut o, p);
    }
    u32_(&mut o, rows.len());
    for r in rows {
        str_(&mut o, &r.id);
        str_(&mut o, &r.name);
        str_(&mut o, &r.model);
        u32_(&mut o, r.plugins.len());
        for &p in &r.plugins {
            u32_(&mut o, p);
        }
        u32_(&mut o, r.placed as usize);
    }
    o
}

/* Wraithguard: `WGTB`, the Editor's other lists as bytes - the same reason as `WGOR`
   (a JSON answer the page cannot read looked like an empty list, Static 2026-10-13), in
   one shape for every list rather than a format each:

   "WGTB" | u32 version 1 | u32 table count | per table: name (string), column types
   (string, one letter a column: `s` string, `u` u32, `i` i32, `d` f64), u32 row count,
   then the rows' values in order. A string is u32 byte length + UTF-8; little-endian.

   The page's `unpackTables` (03_core.js) reads it; `ui/tests/editor/wgtb_tables.json` is
   the one fixture both sides are tested against. The writer does not check a row against
   its column letters: each list is built in one place and the fixture holds the format. */
pub(super) struct Tables {
    o: Vec<u8>,
    tables: u32,
}

impl Tables {
    pub(super) fn new() -> Self {
        let mut o = b"WGTB".to_vec();
        o.extend_from_slice(&1u32.to_le_bytes());
        o.extend_from_slice(&0u32.to_le_bytes()); // the table count, set by `done`
        Tables { o, tables: 0 }
    }
    pub(super) fn table(&mut self, name: &str, cols: &str, rows: usize) -> &mut Self {
        self.tables += 1;
        self.s(name).s(cols);
        self.o.extend_from_slice(&(rows.min(u32::MAX as usize) as u32).to_le_bytes());
        self
    }
    pub(super) fn s(&mut self, v: &str) -> &mut Self {
        self.o.extend_from_slice(&(v.len() as u32).to_le_bytes());
        self.o.extend_from_slice(v.as_bytes());
        self
    }
    pub(super) fn u(&mut self, v: u64) -> &mut Self {
        self.o.extend_from_slice(&(v.min(u32::MAX as u64) as u32).to_le_bytes());
        self
    }
    pub(super) fn i(&mut self, v: i64) -> &mut Self {
        self.o.extend_from_slice(&(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32).to_le_bytes());
        self
    }
    pub(super) fn d(&mut self, v: f64) -> &mut Self {
        self.o.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub(super) fn done(mut self) -> Vec<u8> {
        let n = self.tables.to_le_bytes();
        self.o[8..12].copy_from_slice(&n);
        self.o
    }
}

/// One actor, `"key":{kind, model, twin, health, persistent, corpse, hello?}`, as the
/// cell's `actors` map carries it (and `editor_actors`).
pub(super) fn push_actor(w: &viewcore::world::World, key: &str, a: &viewcore::world::ActorDef, actors: &mut String) {
    actors.push('"');
    viewcore::json::escape_into(key, actors);
    actors.push_str("\":{\"kind\":\"");
    actors.push_str(if a.creature { "crea" } else { "npc" });
    actors.push_str("\",\"model\":\"");
    viewcore::json::escape_into(&a.model, actors);
    // Round 18ag: and the `x` twin the game draws the living creature with.
    actors.push_str("\",\"twin\":");
    match &a.twin {
        Some(t) => {
            actors.push('"');
            viewcore::json::escape_into(t, actors);
            actors.push('"');
        }
        None => actors.push_str("null"),
    }
    actors.push_str(",\"health\":");
    match a.health {
        Some(h) => actors.push_str(&h.to_string()),
        None => actors.push_str("null"),
    }
    actors.push_str(",\"persistent\":");
    actors.push_str(if a.persistent { "true" } else { "false" });
    actors.push_str(",\"corpse\":");
    actors.push_str(if a.corpse() { "true" } else { "false" });
    // Wraithguard: AI Hello, for the greeting-distance overlay.
    if let Some(h) = w.hello.get(key) {
        actors.push_str(&format!(",\"hello\":{}", h));
    }
    actors.push('}');
}

/// A leveled creature list placed as a spawn point, as the cell's `actors` map carries it.
pub(super) fn push_lev(key: &str, model: &str, actors: &mut String) {
    actors.push('"');
    viewcore::json::escape_into(key, actors);
    actors.push_str("\":{\"kind\":\"lev\",\"model\":\"");
    viewcore::json::escape_into(model, actors);
    actors.push_str("\",\"twin\":null,\"health\":null,\"persistent\":false,\"corpse\":false}");
}

/// Wraithguard: what the render window needs to draw objects it has not seen placed (a
/// reference the editor adds): `{actors:{...}, models:{id: mesh}}`, as a cell carries
/// them, for the ids asked.
#[tauri::command(async)]
pub(super) fn editor_actors(ids: Vec<String>, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut actors = String::from("{");
    let mut models = String::from("{");
    for id in ids.iter().take(4096) {
        let key = id.to_ascii_lowercase();
        if let Some(a) = w.actors.get(&key) {
            if actors.len() > 1 {
                actors.push(',');
            }
            push_actor(&w, &key, a, &mut actors);
        } else if let Some(o) = w.objects.get(&key).filter(|o| o.kind == Some(viewcore::objects::ObjKind::LeveledCreature)) {
            if actors.len() > 1 {
                actors.push(',');
            }
            push_lev(&key, &o.model, &mut actors);
        }
        if let Some(m) = w.models.get(&key) {
            if models.len() > 1 {
                models.push(',');
            }
            models.push('"');
            viewcore::json::escape_into(&key, &mut models);
            models.push_str("\":\"");
            viewcore::json::escape_into(m, &mut models);
            models.push('"');
        }
    }
    actors.push('}');
    models.push('}');
    let mut o = J::obj();
    o.raw("actors", &actors).raw("models", &models);
    Ok(o.done())
}

/// The Cell View's object list for one cell (`x,y`, or `int:<name>`): `{name, rows:[[key,
/// id, tag, [x,y,z], plugin]...]}` - every reference not deleted, sorted by id; `key` is
/// the viewport's reference key, `plugin` the file whose version of it wins.
#[tauri::command(async)]
pub(super) fn editor_cell_refs(cell: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let c = cell_refs_of(&w, &cell)?;
    let rows: Vec<String> = c
        .rows
        .iter()
        .map(|r| {
            let mut row = String::from("[\"");
            viewcore::json::escape_into(&r.key, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.id, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.tag, &mut row);
            row.push_str(&format!("\",[{:.1},{:.1},{:.1}],\"", r.pos[0], r.pos[1], r.pos[2]));
            viewcore::json::escape_into(&r.plugin, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.owner, &mut row);
            row.push_str("\"]");
            row
        })
        .collect();
    let mut o = J::obj();
    o.str("name", &c.name).str("refCell", &c.ref_cell).raw("plugins", &string_array(&c.plugins));
    o.raw("rows", &format!("[{}]", rows.join(",")));
    Ok(o.done())
}

/// Wraithguard: what names each of `keys` (the viewport's reference keys) to the Editor, in
/// one pass over the loaded world: `[{cell, origin, refr, plugins} | null]`, in the order
/// asked (`null`: no such reference). `cell` is the cell's key as a plugin's records key it,
/// `origin` the plugin that created the reference, `plugins` those with a record for the
/// cell. A box selection asked `ori` for each object, which looks through the world for it
/// and reads its plugin files for the help text: seconds for an interior's few hundred
/// (gonzo, 2026-11-09).
#[tauri::command(async)]
pub(super) fn editor_ref_names(keys: Vec<String>, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let want: HashMap<String, usize> = keys.iter().enumerate().map(|(i, k)| (k.to_ascii_lowercase(), i)).collect();
    let mut out: Vec<Option<String>> = vec![None; keys.len()];
    let name = |i: u32| w.plugins.get(i as usize).cloned().unwrap_or_default();
    let left = std::cell::Cell::new(want.len());
    let mut look = |num: &viewcore::esp::RefNum, ref_cell: &str, cell_by: &[u32]| {
        let Some(k) = viewcore::refkey::RefKey::of(*num, &w.plugins) else { return };
        let Some(&i) = want.get(&viewcore::refkey::key_text(&k).to_ascii_lowercase()) else { return };
        if out[i].is_some() {
            return;
        }
        let origin = w.plugins.iter().find(|p| p.eq_ignore_ascii_case(&k.plugin)).cloned().unwrap_or(k.plugin);
        let mut o = J::obj();
        o.str("cell", ref_cell).str("origin", &origin).num("refr", num.index as f64);
        o.raw("plugins", &string_array(&cell_by.iter().map(|&p| name(p)).collect::<Vec<_>>()));
        out[i] = Some(o.done());
        left.set(left.get() - 1);
    };
    'cells: {
        for (g, c) in &w.cells {
            let grid = format!("({}, {})", g.0, g.1);
            for n in c.refs.keys() {
                look(n, &grid, &w.history.exterior(*g).touched_by);
            }
            if left.get() == 0 {
                break 'cells;
            }
        }
        for (k, room) in &w.interiors {
            for n in room.refs.keys() {
                look(n, &room.name, &w.history.interior(k).touched_by);
            }
            if left.get() == 0 {
                break 'cells;
            }
        }
    }
    Ok(format!("[{}]", out.into_iter().map(|o| o.unwrap_or_else(|| "null".into())).collect::<Vec<_>>().join(",")))
}

/// Wraithguard: `editor_cell_refs` as bytes (`WGTB`): `meta` (`ss`: name, refCell, one
/// row), `plugins` (`s`) and `rows` (`sssdddss`: key, id, tag, x, y, z, plugin, owner;
/// the position to a tenth, as the JSON gives it).
#[tauri::command(async)]
pub(super) fn editor_cell_refs_bytes(cell: String, state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    Ok(tauri::ipc::Response::new(pack_cell_refs(&cell_refs_of(&w, &cell)?)))
}

/// One cell's references for the Cell View, as `cell_refs_of` lists them.
pub(super) struct CellRefs {
    pub(super) name: String,
    pub(super) ref_cell: String,
    pub(super) plugins: Vec<String>,
    pub(super) rows: Vec<RefRow>,
}

pub(super) struct RefRow {
    pub(super) key: String,
    pub(super) id: String,
    pub(super) tag: String,
    pub(super) pos: [f32; 3],
    pub(super) plugin: String,
    pub(super) owner: String,
}

pub(super) fn pack_cell_refs(c: &CellRefs) -> Vec<u8> {
    // To a tenth exactly as the JSON writes it (`{:.1}`, which rounds a tie to even).
    let tenth = |v: f32| format!("{v:.1}").parse::<f64>().unwrap_or(0.0);
    let mut t = Tables::new();
    t.table("meta", "ss", 1).s(&c.name).s(&c.ref_cell);
    t.table("plugins", "s", c.plugins.len());
    for p in &c.plugins {
        t.s(p);
    }
    t.table("rows", "sssdddss", c.rows.len());
    for r in &c.rows {
        t.s(&r.key).s(&r.id).s(&r.tag).d(tenth(r.pos[0])).d(tenth(r.pos[1])).d(tenth(r.pos[2])).s(&r.plugin).s(&r.owner);
    }
    t.done()
}

/// The Cell View's references for one cell (`x,y`, or `int:<name>`): every reference not
/// deleted, sorted by id, each with its viewport key, its record's type, where it stands,
/// the plugin whose version wins and its owner.
pub(super) fn cell_refs_of(w: &viewcore::world::World, cell: &str) -> Result<CellRefs, String> {
    // Also the cell's key as plugins' records key it, and the plugins with a CELL record
    // for it: what the editor's reference dialog asks Wraithguard with.
    type Cell<'a> = (String, String, &'a [u32], Vec<&'a viewcore::esp::CellRef>);
    let key = viewcore::cells::CellKey::parse(cell).ok_or_else(|| format!("not a cell: {cell}"))?;
    let (name, ref_cell, by, refs): Cell = match &key {
        viewcore::cells::CellKey::Interior(room) => {
            let k = viewcore::world::room_key(room);
            let r = w.interiors.get(&k).ok_or_else(|| format!("no interior {room}"))?;
            (r.name.clone(), r.name.clone(), &w.history.interior(&k).touched_by, r.refs.values().collect())
        }
        &viewcore::cells::CellKey::Exterior(x, y) => {
            let c = w.cells.get(&(x, y)).ok_or_else(|| format!("no exterior cell {x},{y}"))?;
            // The pool's spelling, `(x, y)`: how the Editor's records name the cell.
            (c.name.clone(), key.pool_key(), &w.history.exterior((x, y)).touched_by, c.refs.values().collect())
        }
    };
    let mut refs: Vec<&viewcore::esp::CellRef> = refs.into_iter().filter(|r| !r.deleted).collect();
    refs.sort_by(|a, b| a.id.to_ascii_lowercase().cmp(&b.id.to_ascii_lowercase()).then(a.num.index.cmp(&b.num.index)));
    /* Wraithguard: each reference's owner, as the plugin that last changed it writes it -
       the Construction Set's Ownership column. The world keeps no owners (drawing does not
       need them), so each plugin that last changed some of these is read once for them. */
    let sel = match key {
        viewcore::cells::CellKey::Interior(_) => viewcore::inspect::CellSel::Interior(ref_cell.clone()),
        viewcore::cells::CellKey::Exterior(x, y) => viewcore::inspect::CellSel::Exterior(x, y),
    };
    let mut owners: std::collections::HashMap<i32, std::collections::HashMap<(u32, String), viewcore::inspect::RefDetails>> = std::collections::HashMap::new();
    for r in &refs {
        if owners.contains_key(&r.plugin) {
            continue;
        }
        let got = usize::try_from(r.plugin)
            .ok()
            .and_then(|i| w.plugin_paths.get(i))
            .and_then(|p| std::fs::read(p).ok())
            .map(|buf| viewcore::inspect::cell_details(&buf, &sel))
            .unwrap_or_default();
        owners.insert(r.plugin, got);
    }
    let owner_of = |r: &viewcore::esp::CellRef| -> String {
        let Some(d) = owners.get(&r.plugin).and_then(|m| m.get(&(r.num.index & 0x00ff_ffff, r.id.to_ascii_lowercase()))) else {
            return String::new();
        };
        if !d.owner.is_empty() {
            d.owner.clone()
        } else if !d.faction.is_empty() {
            match d.rank {
                Some(k) => format!("{} (rank {k})", d.faction),
                None => d.faction.clone(),
            }
        } else {
            String::new()
        }
    };
    let rows: Vec<RefRow> = refs
        .iter()
        .map(|r| RefRow {
            key: viewcore::refkey::RefKey::of(r.num, &w.plugins).map(|k| viewcore::refkey::key_text(&k)).unwrap_or_default(),
            id: r.id.clone(),
            tag: w
                .record_where
                .get(&r.id.to_ascii_lowercase())
                .and_then(|d| d.last())
                .map(|(_, t)| tag_text(t))
                .unwrap_or_default(),
            pos: [r.pos[0], r.pos[1], r.pos[2]],
            plugin: plugin_name(w, usize::try_from(r.plugin).ok()).to_string(),
            owner: owner_of(r),
        })
        .collect();
    let plugins: Vec<String> = by.iter().map(|&i| plugin_name(w, Some(i as usize)).to_string()).collect();
    Ok(CellRefs { name, ref_cell, plugins, rows })
}

/// Wraithguard: what kind of record an id is, for the Editor opening one by id alone (a
/// link in the inspector or the full help, whose caller can only guess the type): `{tag,
/// plugins}` - the type the winning plugin gives it, and the plugins that define it as that
/// type, in load order - or `{tag: null}` when no plugin has it.
#[tauri::command(async)]
pub(super) fn editor_record_tag(id: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut o = J::obj();
    match w.record_where.get(&id.trim().to_ascii_lowercase()).and_then(|d| d.last().map(|(_, t)| (d, *t))) {
        Some((defs, tag)) => {
            let mut ixs: Vec<usize> = defs.iter().filter(|(_, t)| *t == tag).map(|(p, _)| *p).collect();
            ixs.dedup();
            let plugins: Vec<String> = ixs.iter().map(|&i| plugin_name(&w, Some(i)).to_string()).collect();
            o.str("tag", &tag_text(&tag)).raw("plugins", &string_array(&plugins));
        }
        None => {
            o.raw("tag", "null").raw("plugins", "[]");
        }
    }
    Ok(o.done())
}

/// Wraithguard: the load order's OpenMW Lua scripts, found and checked by the engine
/// (`viewcore::luascan`, luacore's scan over the overlay - packed scripts included): `{openmw,
/// revision, resources, note, scripts:[{path, flags, file, providers, interface,
/// handlers}], findings:[{path, severity, code, line, message}], report}`. The Editor's
/// Lua panel. A Morrowind.ini setup has none, and says so in `note`.
#[tauri::command(async)]
pub(super) fn lua_scan(state: State) -> Result<String, String> {
    let (vfs, order) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.order_file.clone())
    };
    viewcore::luascan::scan_json(&vfs, order.as_deref())
}

/// The Editor's script flowchart (`editFlowchart` was Wraithguard's, opening its chart
/// window): the control flow of the script as typed, `{heading, chart}` (Mermaid), shown in
/// the viewer's own chart panel.
#[tauri::command(async)]
pub(super) fn editor_script_flowchart(id: String, text: String) -> Result<String, String> {
    let v = serde_json::json!({"heading": format!("{id} - flowchart"), "chart": editcore::mwscript::flowchart::flowchart(&text)});
    Ok(v.to_string())
}

/// Wraithguard: a Lua script's charts, from luacore (`editLuaChart` was Wraithguard's): `kind`
/// `functions` (each function's name and line, to pick from), `flow` (the whole script's
/// flowchart, or the function's at `line`) or `calls` (its call graph). `text` is the script
/// as the page read it. Answers `{functions}` or `{heading, chart}` (Mermaid).
#[tauri::command(async)]
pub(super) fn lua_chart(path: String, text: String, kind: String, line: Option<usize>) -> Result<String, String> {
    use viewcore::luacore::{mermaid, parser};
    let tree = parser::parse(&text, path.to_lowercase().ends_with(".tl")).map_err(|(m, l, c)| format!("{path}:{l}:{c}: {m}"))?;
    // Depth first, as the Python's walk; one per line (a named function's body shares it).
    fn walk<'a>(n: &'a parser::Node, out: &mut Vec<&'a parser::Node>) {
        out.push(n);
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut all = Vec::new();
    walk(&tree, &mut all);
    let mut by_line: std::collections::BTreeMap<usize, &parser::Node> = std::collections::BTreeMap::new();
    for n in all {
        if matches!(n.kind, "Function" | "LocalFunction" | "FunctionStat") {
            by_line.entry(n.line).or_insert(n);
        }
    }
    let name = |n: &parser::Node| n.value.clone().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("function at line {}", n.line));
    match kind.as_str() {
        "functions" => Ok(serde_json::json!({"functions": by_line.values().map(|n| serde_json::json!({"name": name(n), "line": n.line})).collect::<Vec<_>>()}).to_string()),
        "calls" => Ok(serde_json::json!({"heading": format!("{path} - call graph"), "chart": mermaid::call_graph_chart(&tree)}).to_string()),
        "flow" => {
            let target = line.and_then(|l| by_line.get(&l).copied());
            let heading = match target {
                Some(n) => format!("{path} - {}", name(n)),
                None => path.clone(),
            };
            Ok(serde_json::json!({"heading": heading, "chart": mermaid::flowchart(target.unwrap_or(&tree))}).to_string())
        }
        _ => Err(format!("no chart {kind:?}")),
    }
}

/* ---- the Editor, answered here (Wraithguard; port step 4.1, slice 7) ------------------

   The Editor's links answered by the viewer's own engine (`editcore::links`), the same JSON
   in and out as the Python app's loopback handlers were (gui/editorlink.py): the page asks
   `editor_link` for the links `editor_open` names, and only those (the loopback links went
   with the Python app; 2026-11-10). One Editor, for the load order the launch named
   (`extra.editor`) or the install the viewer has open (`editor_setup`). */

pub(super) static EDITOR: Mutex<Option<std::sync::Arc<editcore::links::Links>>> = Mutex::new(None);

/// Wraithguard: the viewer's Editor opened on a load order (`plugins`: `[[name, path]]`, in
/// order) and the journal it shares with Wraithguard (`""`: none) - kept when it is open on
/// those already. `folders`: the setup's data folders, the highest priority first (where
/// a patch would go); `last`: the patch last written (`""`: none); `testSettings`: where
/// Test in OpenMW's choices are kept (`""`: testing refused). The links it answers, as
/// JSON.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub(super) fn editor_open(
    plugins: Vec<(String, String)>,
    journal: String,
    folders: Option<Vec<String>>,
    last: Option<String>,
    test_settings: Option<String>,
    encoding: Option<String>,
) -> Result<String, String> {
    let plugins: Vec<(String, PathBuf)> = plugins.into_iter().map(|(n, p)| (n, PathBuf::from(p))).collect();
    let path = |s: Option<String>| s.filter(|s| !s.is_empty()).map(PathBuf::from);
    let journal = path(Some(journal));
    let mut slot = EDITOR.lock().unwrap_or_else(|p| p.into_inner());
    // The install's code page (`encoding`, as OpenMW names it; none: Windows-1252): the
    // Editor reads and writes the plugins' text in it. Another page is another Editor.
    let before = editcore::plugin::code_page();
    let page = editcore::plugin::set_code_page(encoding.as_deref().unwrap_or(""));
    let same = page == before && slot.as_ref().is_some_and(|l| l.plugins == plugins && l.journal == journal);
    if !same {
        // The Use Report's index beside the journal.
        let use_index = journal.as_ref().and_then(|j| j.parent()).map(|d| d.join("wraithguard_use_index.sqlite3"));
        let record_cache = journal.as_ref().and_then(|j| j.parent()).map(|d| d.join("wraithguard_record_cache.sqlite3"));
        let mut links = editcore::links::Links::open(plugins, journal).with_setup(folders.unwrap_or_default(), path(last), path(test_settings));
        if let Some(p) = use_index {
            links = links.with_use_index(p).start_indexing();
        }
        // What it reads from the plugins is kept between runs, beside the journal as the Python
        // Editor kept it.
        if let Some(p) = record_cache {
            links = links.with_record_cache(p);
        }
        *slot = Some(std::sync::Arc::new(links));
    }
    // Link names are bare words: no escaping to do.
    Ok(format!("[{}]", editcore::links::ANSWERS.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(",")))
}

/// The program on its own (4.6: no Python host hands `extra.editor` over): what the
/// Editor opens on, from the install the viewer has open - `{plugins: [[name, path]],
/// journal, folders, last, testSettings}`, as Wraithguard's `editor_native` said it. The
/// journal and Test in OpenMW's choices sit beside the program under the names the Python
/// app used, so a queue it left opens here unchanged.
#[tauri::command(async)]
pub(super) fn editor_setup(state: State) -> Result<String, String> {
    let (cfg, l) = {
        let app = state.app();
        (app.order_file.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.layout.clone())
    };
    // An MO2 or Morrowind.ini install: the layout it was opened with (Static, 2026-11-10).
    let setup = match l.filter(|_| !viewcore::luascan::is_openmw_cfg(&cfg)) {
        Some(l) => setup_from_layout(&l),
        None => cf_setup(&cfg.to_string_lossy())?,
    };
    let plugins: Vec<(String, String)> = setup
        .locate(&setup.plugins())
        .into_iter()
        .filter_map(|(n, p)| Some((n, p?.to_string_lossy().into_owned())))
        .collect();
    let folders: Vec<String> = setup.data.iter().rev().filter(|d| d.is_dir()).map(|d| d.to_string_lossy().into_owned()).collect();
    let dir = app_dir()?;
    Ok(serde_json::json!({
        "plugins": plugins,
        // The plugins' code page (the cfg's `encoding=`, the edition's own): the Editor's text.
        "encoding": setup.encoding.unwrap_or_default(),
        "journal": dir.join("wraithguard_patch_journal.json"),
        "folders": folders,
        "last": "",
        "testSettings": dir.join("openmw_test.json"),
    })
    .to_string())
}

/// Wraithguard: one Editor link (`name`, its JSON `body`) answered by the viewer's Editor:
/// the answer's text, or the reason it is refused. The verifier looks for the files records
/// name in the install the viewer has open (loose or archived), when it has one.
#[tauri::command(async)]
pub(super) fn editor_link(state: State, name: String, body: String) -> Result<String, String> {
    let links = EDITOR.lock().unwrap_or_else(|p| p.into_inner()).clone().ok_or_else(|| "The viewer's Editor is not open".to_string())?;
    let vfs = if name == "editVerify" { state.app().vfs.clone() } else { None };
    let exists = |path: &str| vfs.as_ref().is_some_and(|v| v.resolve(path).is_some());
    let file_exists: Option<&dyn Fn(&str) -> bool> = if vfs.is_some() { Some(&exists) } else { None };
    // A sound a record names, opened as the game finds it: one that will not decode is an
    // error; a silent or clipped one, or one only OpenMW plays, a warning.
    let sound = |name: &str| -> Option<(&'static str, String)> {
        let v = vfs.as_ref()?;
        let rel = name.trim().replace('\\', "/");
        let path = if rel.to_ascii_lowercase().starts_with("sound/") { rel } else { format!("Sound/{rel}") };
        let loc = viewcore::audio::candidates(&path).into_iter().find_map(|c| v.resolve(&c))?;
        let bytes = v.load(&loc)?;
        match viewcore::audio::decode(&bytes) {
            Err(e) => Some(("error", format!("will not play: {e}"))),
            Ok(d) => d.problem().or_else(|| {
                // OpenMW plays what FFmpeg reads; Morrowind.exe only WAV and MP3.
                matches!(d.container, "ogg" | "flac").then(|| format!("is {}: OpenMW plays it, Morrowind.exe does not", d.container.to_uppercase()))
            })
            .map(|m| ("warning", m)),
        }
    };
    let sound_problem: Option<editcore::verify::SoundProblem> = if vfs.is_some() { Some(&sound) } else { None };
    links.call_with(&name, &body, &editcore::links::Ctx { file_exists, sound_problem })
}

/// The Editor's "Preview it in a new Cell Preview" (`editPreviewPatch`, which Wraithguard
/// used to answer; port step 4.1): a new viewer window on the setup with `path` loaded last,
/// on the plugin's cells. The setup is written beside `cfg` (so its relative paths still
/// read), as `wraithguard_preview_setup.cfg`; the window keeps this one's profile and theme.
#[tauri::command(async)]
pub(super) fn editor_preview_patch(cfg: String, path: String) -> Result<(), String> {
    let target = editcore::buildjob::preview_target(&serde_json::json!({"path": path}))?;
    let cfg_path = PathBuf::from(&cfg);
    let text = std::fs::read_to_string(&cfg_path).map_err(|e| format!("{cfg}: {e}"))?;
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let lines = editcore::buildjob::with_extra_plugins(&lines, std::slice::from_ref(&target));
    let setup = cfg_path.with_file_name("wraithguard_preview_setup.cfg");
    write_path_whole(&setup.to_string_lossy(), (lines.join("\n") + "\n").as_bytes())?;
    let exe = self_exe()?;
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut args: Vec<String> = vec!["--cell-viewer".into(), "--openmw-cfg".into(), setup.to_string_lossy().into_owned()];
    let mine: Vec<String> = std::env::args().collect();
    for keep in ["--prefs", "--theme"] {
        if let Some(v) = mine.iter().position(|a| a == keep).and_then(|i| mine.get(i + 1)) {
            args.extend([keep.to_string(), v.clone()]);
        }
    }
    if !name.is_empty() && !name.contains(['\r', '\n', '\0']) {
        args.extend(["--cell".to_string(), format!("plugin:{name}")]);
    }
    args.extend(["--title".to_string(), "Cell Preview".to_string()]);
    std::process::Command::new(exe).args(&args).spawn().map(|_| ()).map_err(|e| format!("a new Cell Preview could not be started: {e}"))
}
