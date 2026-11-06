//! The Editor's world: placed objects (the reference dialog and its changes), the
//! references the patch adds, path grids, and the cells and land the patch makes -
//! `wraithguard/patch/editor.py`'s `ref_view`, `set_ref_field` (with `coerce_ref`),
//! `revert_ref`, `place`, `place_many`, `new_view`, `set_new_field`, `remove_new`,
//! `pathgrid_view`, `set_pathgrid`, `revert_pathgrid`, `new_cell`, `new_land` (with
//! `newland.flat_land`) and `duplicate_cell`, rule for rule and message for message (port
//! step 4.1, slice 4b). Cases: `tests/fixtures/place_cases.json`, run here and by
//! `tests/test_place_cases.py`.
//!
//! The patch keeps what it makes in the Python app's form (a path grid's links as a list
//! of numbers), which the writer packs (slice 6).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ordercore::cells::CellKey;
use serde_json::{json, Map, Value};

use crate::edit::{set_at, EditError, Editor};
use crate::key::{record_key, type_of};
use crate::pyval::{self, Kind};
use crate::queue::{Choice, NewRecord, NewRef, RefEdit};
use crate::session::{Found, FoundRef};

/// What each changeable field of a reference holds, in the order the dialog shows them:
/// `vec3` three numbers (rotation in radians), `grid` an exterior's `[x, y]`, `door` a
/// teleport door's destination `{translation, rotation, cell}`.
pub const REF_KINDS: [(&str, &str); 19] = [
    ("translation", "vec3"),
    ("rotation", "vec3"),
    ("scale", "float"),
    ("deleted", "bool"),
    ("owner", "str"),
    ("owner_global", "str"),
    ("owner_faction", "str"),
    ("owner_faction_rank", "int"),
    ("lock_level", "int"),
    ("key", "str"),
    ("trap", "str"),
    ("soul", "str"),
    ("charge_left", "int"),
    ("health_left", "int"),
    ("object_count", "int"),
    ("blocked", "int"),
    ("temporary", "bool"),
    ("moved_cell", "grid"),
    ("destination", "door"),
];

/// The fields of a reference the patch may change (`refedit.REF_FIELDS`).
pub const REF_FIELDS: [&str; 19] = [
    "translation", "rotation", "scale", "deleted", "moved_cell", "owner", "owner_global", "owner_faction",
    "owner_faction_rank", "lock_level", "key", "trap", "soul", "charge_left", "health_left", "object_count", "blocked",
    "temporary", "destination",
];

/// An exterior cell's side, in units.
pub const CELL_SIZE: f64 = 8192.0;

/// The scale the game allows a reference (the Construction Set's limits).
const SCALE: (f64, f64) = (0.5, 2.0);

/// The tags placed persistent: the game expects actors to be.
const PERSISTENT_TAGS: [&str; 2] = ["NPC_", "CREA"];

/// The longest cell name the game reads (64 bytes with the terminator).
pub const MAX_CELL_NAME: usize = 63;

/// Heights flat land may sit at, in units from the sea's level.
pub const MAX_HEIGHT: f64 = 30000.0;
/// How far from the sea's level a shaped height may go: what `VHGT`'s float offset and
/// byte steps can carry across a cell, with room to spare.
const LAND_HEIGHT_LIMIT: f64 = 1.0e6;

fn err<T>(m: impl Into<String>) -> Result<T, EditError> {
    Err(EditError(m.into()))
}

/// The exterior cell a world position is in.
pub fn grid_of(position: &[f64]) -> (i64, i64) {
    ((position[0] / CELL_SIZE).floor() as i64, (position[1] / CELL_SIZE).floor() as i64)
}

/// A cell as the pool keys it: an interior's name (`int:` dropped), an exterior's
/// `(x, y)`, from any spelling; `""` for none.
pub fn cell_key(cell: &str) -> String {
    CellKey::parse(cell).map(|k| k.pool_key()).unwrap_or_default()
}

/// An exterior cell's grid from any spelling, or `None`.
pub fn exterior_grid(cell: &str) -> Option<(i32, i32)> {
    CellKey::parse(cell)?.grid()
}

/// Python's `float()` of a JSON value: a number, a bool (1.0, 0.0), or text.
fn float_value(v: &Value) -> Option<f64> {
    match v {
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Number(n) => n.as_f64(),
        Value::String(s) => pyval::float_of(s),
        _ => None,
    }
}

/// Three finite numbers from what the viewer sent.
fn vec3(raw: Option<&Value>, what: &str) -> Result<Vec<f64>, EditError> {
    let Some(a) = raw.and_then(Value::as_array).filter(|a| a.len() == 3) else {
        return err(format!("{what} is three numbers"));
    };
    let out: Option<Vec<f64>> = a.iter().map(float_value).collect();
    let Some(out) = out else { return err(format!("{what} is three numbers")) };
    if !out.iter().all(|v| v.is_finite()) {
        return err(format!("{what} must be finite"));
    }
    Ok(out)
}

/// A whole number from what the viewer sent.
fn whole(raw: &Value, what: &str) -> Result<i128, EditError> {
    match pyval::float_of(&pyval::str_of(raw)).filter(|n| n.is_finite() && n.fract() == 0.0) {
        Some(n) => Ok(n as i128),
        None => err(format!("{} is not a whole number ({what})", pyval::repr(raw))),
    }
}

fn floats(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|x| pyval::float_value(*x)).collect())
}

/// A value the viewer sent for a reference's field, as the field holds it. An empty
/// value clears an optional field (`null`: the reference does not have it).
pub fn coerce_ref(name: &str, raw: &Value) -> Result<Value, EditError> {
    let Some(kind) = REF_KINDS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k).filter(|_| REF_FIELDS.contains(&name)) else {
        return err(format!("{name} cannot be changed on a reference here"));
    };
    match kind {
        "vec3" => return Ok(floats(&vec3(Some(raw), name)?)),
        "door" => {
            if raw.is_null() || raw.as_str() == Some("") {
                return Ok(Value::Null);
            }
            let Value::Object(d) = raw else {
                return err("a destination is {translation, rotation, cell}");
            };
            let cell = d.get("cell").cloned().unwrap_or(json!(""));
            let Value::String(cell) = cell else {
                return err("a destination's cell is a name (empty: the exterior)");
            };
            let rotation = d.get("rotation").cloned().unwrap_or(json!([0.0, 0.0, 0.0]));
            return Ok(json!({
                "translation": floats(&vec3(d.get("translation"), "the destination's position")?),
                "rotation": floats(&vec3(Some(&rotation), "the destination's rotation")?),
                "cell": cell.trim(),
            }));
        }
        "bool" => {
            if let Value::Bool(b) = raw {
                return Ok(Value::Bool(*b));
            }
            return match pyval::str_of(raw).trim().to_lowercase().as_str() {
                "true" | "1" | "yes" => Ok(json!(true)),
                "false" | "0" | "no" | "" => Ok(json!(false)),
                _ => err(format!("{} is not true or false", pyval::repr(raw))),
            };
        }
        _ => {}
    }
    if raw.is_null() || raw.as_str().is_some_and(|s| s.trim().is_empty()) {
        return Ok(Value::Null);
    }
    match kind {
        "str" => Ok(json!(pyval::str_of(raw))),
        "grid" => {
            let Some(a) = raw.as_array().filter(|a| a.len() == 2) else {
                return err("moved_cell is an exterior's grid, [x, y]");
            };
            Ok(json!([pyval::int_value(whole(&a[0], name)?), pyval::int_value(whole(&a[1], name)?)]))
        }
        "float" => {
            let Some(value) = pyval::float_of(&pyval::str_of(raw)) else {
                return err(format!("{} is not a number", pyval::repr(raw)));
            };
            if name == "scale" && !(SCALE.0..=SCALE.1).contains(&value) {
                return err("scale is 0.5 to 2.0");
            }
            Ok(pyval::float_value(value))
        }
        _ => {
            let value = whole(raw, name)?;
            if !(i32::MIN as i128..=i32::MAX as i128).contains(&value) {
                return err(format!("{value} is out of range"));
            }
            Ok(pyval::int_value(value))
        }
    }
}

/// Whether two field values are the same, floats to within a millionth (a bool is only
/// ever the same as itself).
pub fn same_value(a: &Value, b: &Value) -> bool {
    match (pyval::kind(a), pyval::kind(b)) {
        (Kind::Bool, _) | (_, Kind::Bool) => a == b,
        (Kind::Int | Kind::Float, Kind::Int | Kind::Float) => (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() < 1e-6,
        (Kind::List, Kind::List) => {
            let (x, y) = (a.as_array().unwrap(), b.as_array().unwrap());
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_value(p, q))
        }
        (Kind::Dict, Kind::Dict) => {
            let (x, y) = (a.as_object().unwrap(), b.as_object().unwrap());
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same_value(v, w)))
        }
        _ => pyval::eq(a, b),
    }
}

/// Packed bytes (base64, zstd'd or not), as a reader of tes3conv's JSON gets them.
fn unpack(text: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::STANDARD.decode(text.trim()).ok()?;
    if raw.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) { zstd::decode_all(raw.as_slice()).ok() } else { Some(raw) }
}

/// A path grid's links: a list of point numbers, or the packed `PGRC` (whose `u32` count
/// prefix tes3conv adds is taken off when it reads as one).
fn connections(raw: &Value, expected: usize) -> Vec<i64> {
    match raw {
        Value::Array(a) => a.iter().map(|v| v.as_f64().map(|f| f as i64).or_else(|| pyval::float_of(&pyval::str_of(v)).map(|f| f as i64)).unwrap_or(0)).collect(),
        Value::String(s) => {
            let Some(bytes) = unpack(s).filter(|b| b.len() % 4 == 0) else { return Vec::new() };
            let edges: Vec<i64> = bytes.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c) as i64).collect();
            if !edges.is_empty() && edges[0] == edges.len() as i64 - 1 && expected == edges.len() - 1 {
                edges[1..].to_vec()
            } else {
                edges
            }
        }
        _ => Vec::new(),
    }
}

/// Packs bytes as the crate's serde does a fixed-size array: base64 of a zstd frame.
fn pack(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(zstd::encode_all(bytes, 0).expect("zstd into memory"))
}

/// Where an exterior square with no land is: the flat ground the Construction Set and
/// OpenMW show under the sea there (OpenMW's `ESM::Land::DEFAULT_HEIGHT`), and where new
/// land starts, so it joins the empty squares around it with no step.
pub const DEFAULT_LAND_HEIGHT: f64 = -2048.0;

/// Flat land for an exterior square, in tes3conv's form: every vertex of the 65 x 65
/// grid at one height (0 is the sea's level), normals straight up, one texture over
/// all 256 texture cells (`texture_index`: 0 the default ground, else a land texture's
/// index plus one). VHGT's offset is stored in eighths of a unit, as the game reads it.
pub fn flat_land(grid: (i32, i32), height: f64, texture_index: i64) -> Result<Value, EditError> {
    if !height.is_finite() || height.abs() > MAX_HEIGHT {
        return err(format!("the height is within {MAX_HEIGHT} units of the sea's level"));
    }
    if !(0..=0xFFFF).contains(&texture_index) {
        return err("the texture index does not fit a land cell's texture entry");
    }
    const VERTICES: usize = 65 * 65;
    let coarse = (height / 128.0).round_ties_even().clamp(-128.0, 127.0) as i8;
    let normals: Vec<u8> = [0u8, 0, 127].repeat(VERTICES);
    let textures: Vec<u8> = (0..256).flat_map(|_| (texture_index as u16).to_le_bytes()).collect();
    Ok(json!({
        "type": "Landscape",
        "flags": "",
        "grid": [grid.0, grid.1],
        "landscape_flags": "USES_VERTEX_HEIGHTS_AND_NORMALS | USES_TEXTURES",
        "vertex_normals": {"data": pack(&normals)},
        "vertex_heights": {"offset": pyval::float_value(height / 8.0), "data": pack(&vec![0u8; VERTICES])},
        "world_map_data": {"data": pack(&[coarse as u8; 81])},
        "vertex_colors": {"data": pack(&vec![0u8; VERTICES * 3])},
        "texture_indices": {"data": pack(&textures)},
    }))
}

/// A new reference's editor name: `new-` and eight hex digits, unique in this process.
fn token() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(NEXT.fetch_add(1, Ordering::Relaxed));
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!("new-{:08x}", h.finish() as u32)
}

fn ref_fields(get: impl Fn(&str) -> Value) -> Vec<Value> {
    REF_KINDS.iter().map(|(name, kind)| json!({"path": name, "value": get(name), "kind": kind, "editable": true})).collect()
}

impl Editor {
    // -- placed objects -------------------------------------------------------------

    fn ref_queued(&self, found: &FoundRef) -> Map<String, Value> {
        let id = (found.cell.to_lowercase(), found.origin.to_lowercase(), found.refr_index);
        self.queue.ref_edits().find(|e| e.ident() == id).map(|e| e.changes.to_object().as_object().cloned().unwrap_or_default()).unwrap_or_default()
    }

    /// What the reference dialog shows: `{cell, origin, refr, id, winner, plugins,
    /// fields, warning}`, each field `{path, value, kind, editable}` with `queued` when a
    /// change waits for it (`value` null for a field the reference does not have);
    /// `warning` says a change to a deleted object has no effect, or is null.
    pub fn ref_view(&self, found: &FoundRef) -> Value {
        let queued = self.ref_queued(found);
        let fields: Vec<Value> = REF_KINDS
            .iter()
            .map(|(name, kind)| {
                let mut item = json!({"path": name, "value": found.reference.get(*name).cloned().unwrap_or(Value::Null), "kind": kind, "editable": true});
                if let Some(q) = queued.get(*name) {
                    item["queued"] = q.clone();
                }
                item
            })
            .collect();
        // A deletion sticks (`ordercore::refs`): the game keeps the object, hidden, so a change
        // saved in the patch edits a hidden object and shows nothing.
        let warning = if found.reference.get("deleted").is_some_and(crate::session::truthy) {
            json!(format!("{} deletes this object. A change saved in the patch has no effect: the game keeps it deleted.", found.winner))
        } else {
            Value::Null
        };
        json!({
            "cell": found.cell,
            "origin": found.origin,
            "refr": found.refr_index,
            "id": found.reference.get("id").cloned().unwrap_or(json!("")),
            "winner": found.winner,
            "plugins": found.plugins,
            "fields": fields,
            "warning": warning,
        })
    }

    /// Queues a change to a placed object (and saves the journal); a value equal to what
    /// the load order has drops the change. One moved out of its exterior cell says where
    /// it is now (MVRF), or no longer once back.
    pub fn set_ref_field(&mut self, found: &FoundRef, path: &str, raw: &Value) -> Result<(), EditError> {
        let value = coerce_ref(path, raw)?;
        self.queue_ref(found, path, value.clone());
        if path == "translation"
            && let (Some(home), Some(a)) = (exterior_grid(&found.cell), value.as_array())
        {
            let pos: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            let now = grid_of(&pos);
            let moved = if now == (home.0 as i64, home.1 as i64) { Value::Null } else { json!([now.0, now.1]) };
            self.queue_ref(found, "moved_cell", moved);
        }
        self.save_journal();
        Ok(())
    }

    fn queue_ref(&mut self, found: &FoundRef, path: &str, value: Value) {
        if same_value(&value, found.reference.get(path).unwrap_or(&Value::Null)) {
            self.queue.remove_ref_edit(&found.cell, &found.origin, found.refr_index, Some(path));
        } else {
            let mut changes = crate::queue::Fields::default();
            changes.insert(path.to_string(), value);
            self.queue.add_ref_edit(RefEdit { cell: found.cell.clone(), origin: found.origin.clone(), refr_index: found.refr_index, changes, plugins: found.plugins.clone() });
        }
    }

    /// Drops the change to one field of a placed object, or all of them (a move takes the
    /// cell it moved to with it).
    /// Takes out of the patch the changes queued for a placed object no plugin has now
    /// (`editRefRevert` when the object cannot be found). `false`: none queued.
    pub fn drop_orphan_ref(&mut self, cell: &str, origin: &str, refr: i64) -> bool {
        let Some((c, o, r)) = self
            .queue
            .ref_edits()
            .find(|e| e.cell.eq_ignore_ascii_case(cell.trim()) && e.origin.eq_ignore_ascii_case(origin.trim()) && e.refr_index == refr)
            .map(|e| (e.cell.clone(), e.origin.clone(), e.refr_index))
        else {
            return false;
        };
        self.queue.remove_ref_edit(&c, &o, r, None);
        self.save_journal();
        true
    }

    pub fn revert_ref(&mut self, found: &FoundRef, path: Option<&str>) {
        self.queue.remove_ref_edit(&found.cell, &found.origin, found.refr_index, path);
        if path == Some("translation") {
            self.queue.remove_ref_edit(&found.cell, &found.origin, found.refr_index, Some("moved_cell"));
        }
        self.save_journal();
    }

    // -- new references --------------------------------------------------------------

    /// The plugins with a CELL record for a cell, in load order: the viewer's list,
    /// checked, or every plugin looked in. None for the patch's own cell.
    pub fn cell_plugins(&self, cell: &str, plugins: Option<&[String]>) -> Vec<String> {
        let plugins = plugins.filter(|p| !p.is_empty());
        if plugins.is_none() && self.queue.new_record("Cell", cell.trim()).is_some() {
            return Vec::new();
        }
        let want = cell.trim().to_lowercase();
        if plugins.is_none() {
            // The plugins' own cell names and grids say which have it; no cell is parsed.
            if let Some(holders) = self.session.cell_holders(&want) {
                return holders;
            }
            self.session.warm(&["CELL"]);
        }
        self.session
            .known(plugins)
            .into_iter()
            .filter(|p| self.session.records(p, "CELL").contains_key(&want))
            .map(|p| self.session.spelled(&p).to_string())
            .collect()
    }

    /// Queues a new reference to a record (and saves the journal).
    pub fn place(&mut self, cell: &str, found: &Found, translation: &Value, rotation: Option<&Value>, plugins: Option<&[String]>) -> Result<NewRef, EditError> {
        let holders = self.cell_plugins(cell, plugins);
        let base = if found.new { String::new() } else { found.winner().to_string() };
        let rec = if let Some(last) = holders.last() {
            self.session.records(last, "CELL").get(&cell.trim().to_lowercase()).cloned().unwrap_or(Value::Null)
        } else if let Some(own) = self.queue.new_record("Cell", &cell_key(cell)) {
            own.record.clone()
        } else {
            return err(format!("no plugin of this load order has the cell {cell}"));
        };
        let mut fields = crate::queue::Fields::default();
        fields.insert("id".into(), json!(found.key));
        fields.insert("translation".into(), coerce_ref("translation", translation)?);
        fields.insert("rotation".into(), coerce_ref("rotation", rotation.unwrap_or(&json!([0, 0, 0])))?);
        fields.insert("temporary".into(), json!(!PERSISTENT_TAGS.contains(&found.tag.as_str())));
        let new = NewRef { cell: record_key(&rec), uid: token(), fields, base_plugin: base, plugins: holders, tag: found.tag.clone() };
        self.queue.add_new_ref(new.clone());
        self.save_journal();
        Ok(new)
    }

    /// Several new references at once (a prefab), as one change: `(record, translation,
    /// rotation, scale)` each; in an exterior, each goes in the square it stands in.
    pub fn place_many(&mut self, cell: &str, items: &[(Found, Value, Option<Value>, f64)]) -> Result<Vec<NewRef>, EditError> {
        self.batch(|ed| {
            let mut placed = Vec::new();
            for (found, translation, rotation, scale) in items {
                let mut into = cell.to_string();
                if exterior_grid(&cell_key(cell)).is_some() {
                    let t = coerce_ref("translation", translation)?;
                    let pos: Vec<f64> = t.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                    let (gx, gy) = grid_of(&pos);
                    into = format!("({gx}, {gy})");
                }
                let mut new = ed.place(&into, found, translation, rotation.as_ref(), None)?;
                if *scale != 0.0 && (scale - 1.0).abs() > 1e-6 {
                    new = ed.set_new_field(&new, "scale", &pyval::float_value(*scale))?;
                }
                placed.push(new);
            }
            Ok(placed)
        })
    }

    /// A queued new reference, by its editor name (the cell may have moved under it).
    pub fn find_new(&self, uid: &str) -> Option<NewRef> {
        self.queue.new_refs().find(|n| n.uid == uid).cloned()
    }

    /// The reference dialog for a new reference: its fields as they will be written.
    pub fn new_view(&self, new: &NewRef) -> Value {
        json!({
            "cell": new.cell,
            "uid": new.uid,
            "id": new.fields.get(&"id".to_string()).cloned().unwrap_or(json!("")),
            "tag": new.tag,
            "new": true,
            "plugins": new.plugins,
            "fields": ref_fields(|name| new.fields.get(&name.to_string()).cloned().unwrap_or(Value::Null)),
        })
    }

    /// Changes a field of a new reference (and saves the journal); one moved into another
    /// exterior square goes to that square's cell.
    pub fn set_new_field(&mut self, new: &NewRef, path: &str, raw: &Value) -> Result<NewRef, EditError> {
        let value = coerce_ref(path, raw)?;
        let mut new = new.clone();
        if path == "translation"
            && let (Some(home), Some(a)) = (exterior_grid(&new.cell), value.as_array())
        {
            let pos: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            let now = grid_of(&pos);
            if now != (home.0 as i64, home.1 as i64) {
                let cell = format!("({}, {})", now.0, now.1);
                let holders = self.cell_plugins(&cell, None);
                if holders.is_empty() {
                    return err(format!("no plugin of this load order has the cell {cell}"));
                }
                self.queue.remove_new_ref(&new.cell, &new.uid);
                new = NewRef { cell, plugins: holders, ..new };
                self.queue.add_new_ref(new.clone());
            }
        }
        let mut fields = crate::queue::Fields::default();
        fields.insert(path.to_string(), value);
        self.queue.add_new_ref(NewRef { cell: new.cell.clone(), uid: new.uid.clone(), fields, base_plugin: String::new(), plugins: Vec::new(), tag: String::new() });
        self.save_journal();
        Ok(self.find_new(&new.uid).unwrap_or(new))
    }

    /// Drops a new reference from the patch (and saves the journal).
    pub fn remove_new(&mut self, new: &NewRef) {
        self.queue.remove_new_ref(&new.cell, &new.uid);
        self.save_journal();
    }

    // -- cells and land ----------------------------------------------------------------

    /// Whether the patch or any plugin has a record.
    pub fn in_load_order(&self, tag: &str, rid: &str) -> bool {
        let Some(record_type) = type_of(tag) else { return false };
        self.queue.new_record(record_type, rid.trim()).is_some() || self.find(tag, rid, None).is_some()
    }

    /// A cell of the patch's own, from nothing (and the journal saved): a blank interior
    /// under a name (water off, a dim grey light), or an exterior square the load order
    /// does not have. `absent`: the viewer has looked already, so the load order is not
    /// read again. `{cell, interior}`.
    pub fn new_cell(&mut self, cell: &str, absent: bool) -> Result<Value, EditError> {
        let key = cell_key(cell);
        let grid = exterior_grid(&key);
        if key.is_empty() {
            return err("a new cell needs a name, or (x, y) for an exterior square");
        }
        if self.queue.new_record("Cell", &key).is_some() || (!absent && self.in_load_order("CELL", &key)) {
            return err(format!("the load order has the cell {key} already"));
        }
        let record = match grid {
            Some((x, y)) => json!({"type": "Cell", "flags": "", "name": "", "data": {"flags": "", "grid": [x, y]}, "references": []}),
            None => {
                if key.chars().count() > MAX_CELL_NAME {
                    return err(format!("a cell name is at most {MAX_CELL_NAME} characters"));
                }
                json!({
                    "type": "Cell", "flags": "", "name": key, "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
                    "atmosphere_data": {"ambient_color": [64, 64, 64, 0], "sunlight_color": [64, 64, 64, 0], "fog_color": [0, 0, 0, 0], "fog_density": 1.0},
                    "references": [],
                })
            }
        };
        self.queue.add_new_record(NewRecord { record_type: "Cell".into(), key: key.clone(), record, source: String::new(), topic: String::new() });
        self.save_journal();
        Ok(json!({"cell": key, "interior": grid.is_none()}))
    }

    /// Flat land for an exterior square the load order has none for (and the journal
    /// saved), its cell made with it when no one has that. `{cell, land, madeCell}`.
    pub fn new_land(&mut self, cell: &str, height: f64, texture: &str, no_land: bool, has_cell: Option<bool>) -> Result<Value, EditError> {
        let Some(grid) = exterior_grid(&cell_key(cell)) else {
            return err("land belongs to an exterior square: give (x, y)");
        };
        let key = format!("({}, {})", grid.0, grid.1);
        if self.queue.new_record("Landscape", &key).is_some() || (!no_land && self.in_load_order("LAND", &key)) {
            return err(format!("{key} has land already"));
        }
        let (mut index, mut source) = (0, String::new());
        if !texture.trim().is_empty() {
            let Some(found) = self.find("LTEX", texture.trim(), None) else {
                return err(format!("no plugin has a land texture {}", texture.trim()));
            };
            let i = found.record().get("index").and_then(Value::as_f64).unwrap_or(0.0);
            index = i as i64 + 1;
            source = found.winner().to_string();
        }
        let record = flat_land(grid, height, index)?;
        let made_cell = match has_cell {
            None => !self.in_load_order("CELL", &key),
            Some(has) => !has && self.queue.new_record("Cell", &key).is_none(),
        };
        self.batch(|ed| {
            if made_cell {
                ed.new_cell(&key, true)?;
            }
            ed.queue.add_new_record(NewRecord { record_type: "Landscape".into(), key: key.clone(), record, source, topic: String::new() });
            ed.save_journal();
            Ok(())
        })?;
        Ok(json!({"cell": key, "land": true, "madeCell": made_cell}))
    }

    /// A cell's heights set (land editing, the Land view's brushes): `heights` the 65 x 65
    /// vertex heights in world units, row by row from the south-west. The record's normals
    /// and world map are derived from them (`landcore::emit`, as Merge Lands writes them);
    /// its vertex colors and textures are kept. The land must be there already (the load
    /// order's, or the patch's own from New land). A step too steep for the format is
    /// clamped, as the game's own encoding would. `{cell, clamped}`.
    pub fn set_land(&mut self, cell: &str, heights: &[f64]) -> Result<Value, EditError> {
        let Some(grid) = exterior_grid(&cell_key(cell)) else {
            return err("land belongs to an exterior square: give (x, y)");
        };
        let key = format!("({}, {})", grid.0, grid.1);
        if heights.len() != 65 * 65 {
            return err(format!("a cell has 4225 heights, not {}", heights.len()));
        }
        if heights.iter().any(|h| !h.is_finite() || h.abs() > LAND_HEIGHT_LIMIT) {
            return err(format!("every height is a number within {LAND_HEIGHT_LIMIT} units of the sea's level"));
        }
        let Some(found) = self.find("LAND", &key, None) else {
            return err(format!("{key} has no land to shape: make land there first (New land)"));
        };
        let whole: Vec<landcore::grid::Value> = heights.iter().map(|h| h.round_ties_even() as landcore::grid::Value).collect();
        let (built, clamped) = landcore::emit::build_landscape_record((grid.0 as i64, grid.1 as i64), Some(&whole), None, None, None).map_err(EditError)?;
        let as_json = serde_json::to_value(tes3::esp::TES3Object::Landscape(built)).map_err(|e| EditError(e.to_string()))?;
        let mut fields: Vec<(&str, Value)> = ["vertex_heights", "vertex_normals", "world_map_data"].iter().map(|f| (*f, as_json[*f].clone())).collect();
        let flags = found.record().get("landscape_flags").and_then(Value::as_str).unwrap_or("").to_string();
        if !flags.contains("USES_VERTEX_HEIGHTS_AND_NORMALS") {
            let with = if flags.trim().is_empty() { "USES_VERTEX_HEIGHTS_AND_NORMALS".to_string() } else { format!("{flags} | USES_VERTEX_HEIGHTS_AND_NORMALS") };
            fields.push(("landscape_flags", json!(with)));
        }
        if found.new {
            let Some(made) = self.queue.new_record(&found.record_type, &found.key).cloned() else {
                return err(format!("{} is no longer in the patch", found.key));
            };
            let mut record = made.record.clone();
            for (path, value) in fields {
                record[path] = value;
            }
            self.queue.add_new_record(NewRecord { record, ..made });
        } else {
            for (path, value) in fields {
                self.queue.add_field(&found.record_type, &found.key, Choice::Value { path: path.into(), value });
            }
            self.queue.set_base(&found.record_type, &found.key, found.winner());
        }
        self.save_journal();
        Ok(json!({"cell": key, "clamped": clamped.len()}))
    }

    /// Every cell whose land the patch changes, as the Land view draws it over the load
    /// order's (after a reconnect, a restart with the journal, or an undo): `[{cell, heights?,
    /// colors?, ids?}]`, each layer only when the patch carries it. `ids` are the texture
    /// cells' land texture ids, row by row; a texture number the patch does not define is
    /// `null` there (the load order's drawing is kept for that cell's textures then).
    pub fn land_pending(&mut self) -> Value {
        let mut keys: Vec<(String, bool)> = Vec::new();
        for ((rtype, key), _) in self.queue.fields().iter() {
            if rtype == "Landscape" && !keys.iter().any(|(k, _)| k == key) {
                keys.push((key.clone(), false));
            }
        }
        for r in self.queue.new_records().filter(|r| r.record_type == "Landscape") {
            if !keys.iter().any(|(k, _)| *k == r.key) {
                keys.push((r.key.clone(), true));
            }
        }
        let ltex: HashMap<i64, String> = self
            .queue
            .new_records()
            .filter(|r| r.record_type == "LandscapeTexture")
            .filter_map(|r| r.record.get("index").and_then(Value::as_i64).map(|i| (i, r.key.clone())))
            .collect();
        let mut out = Vec::new();
        for (key, whole) in keys {
            let Some(found) = self.find("LAND", &key, None) else { continue };
            let mut record = found.record().clone();
            let mut changed: Vec<String> = Vec::new();
            if let (false, Some(choices)) = (found.new, self.queue.fields().get(&("Landscape".to_string(), key.clone()))) {
                for c in choices {
                    if let Choice::Value { path, value } = c {
                        record[path.as_str()] = value.clone();
                        changed.push(path.clone());
                    }
                }
            }
            let has = |f: &str| whole || found.new || changed.iter().any(|c| c == f);
            let Ok(tes3::esp::TES3Object::Landscape(l)) = serde_json::from_value::<tes3::esp::TES3Object>(record) else { continue };
            let mut o = json!({"cell": key});
            if has("vertex_heights") {
                let raw: Vec<u8> = l.vertex_heights.data.iter().flatten().map(|&d| d as u8).collect();
                if let Ok(h) = landcore::heights::decode_heights(&raw, l.vertex_heights.offset as f64) {
                    o["heights"] = json!(h);
                }
            }
            if has("vertex_colors") {
                o["colors"] = json!(l.vertex_colors.data.iter().flatten().flatten().copied().collect::<Vec<u8>>());
            }
            if has("texture_indices") {
                let raw: Vec<u8> = l.texture_indices.data.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
                let ids: Vec<Value> = landcore::layers::decode_texture_indices(&raw)
                    .into_iter()
                    .map(|v| if v == 0 { json!("_land_default") } else { ltex.get(&(v as i64 - 1)).map(|id| json!(id)).unwrap_or(Value::Null) })
                    .collect();
                o["ids"] = Value::Array(ids);
            }
            out.push(o);
        }
        Value::Array(out)
    }

    /// A cell's vertex colors set (the Land view's color brush): `colors` the 65 x 65
    /// vertices' red, green and blue, 0-255 each, row by row from the south-west - the tint
    /// the game multiplies the ground's textures by. `{cell}`.
    pub fn set_land_colors(&mut self, cell: &str, colors: &[f64]) -> Result<Value, EditError> {
        let Some(grid) = exterior_grid(&cell_key(cell)) else {
            return err("land belongs to an exterior square: give (x, y)");
        };
        let key = format!("({}, {})", grid.0, grid.1);
        if colors.len() != 65 * 65 * 3 {
            return err(format!("a cell has 12675 color values (red, green, blue per vertex), not {}", colors.len()));
        }
        let Some(found) = self.find("LAND", &key, None) else {
            return err(format!("{key} has no land to color: make land there first (New land)"));
        };
        let whole: Vec<landcore::grid::Value> = colors.iter().map(|c| if c.is_finite() { c.round_ties_even().clamp(0.0, 255.0) as landcore::grid::Value } else { 255 }).collect();
        let (built, _) = landcore::emit::build_landscape_record((grid.0 as i64, grid.1 as i64), None, None, None, Some(&whole)).map_err(EditError)?;
        let as_json = serde_json::to_value(tes3::esp::TES3Object::Landscape(built)).map_err(|e| EditError(e.to_string()))?;
        let mut fields: Vec<(&str, Value)> = vec![("vertex_colors", as_json["vertex_colors"].clone())];
        let flags = found.record().get("landscape_flags").and_then(Value::as_str).unwrap_or("").to_string();
        if !flags.contains("USES_VERTEX_COLORS") {
            let with = if flags.trim().is_empty() { "USES_VERTEX_COLORS".to_string() } else { format!("{flags} | USES_VERTEX_COLORS") };
            fields.push(("landscape_flags", json!(with)));
        }
        self.queue_land_fields(&found, fields)?;
        Ok(json!({"cell": key}))
    }

    /// The region map's brush (TODO, "The world"): `region` (a region id; empty clears it)
    /// set on each exterior square of `cells`, as the cell's `region` field - over the load
    /// order's record, or on the patch's own. A square with no cell anywhere gets one of the
    /// patch's own first when `make` (else it is left and counted). `{set, made, skipped}`.
    pub fn set_cell_regions(&mut self, cells: &[String], region: &str, make: bool) -> Result<Value, EditError> {
        let region = region.trim();
        let value = if region.is_empty() {
            Value::Null
        } else {
            let Some(found) = self.find("REGN", region, None) else {
                return err(format!("no plugin has a region {region}"));
            };
            json!(found.record().get("id").and_then(Value::as_str).unwrap_or(region))
        };
        let (mut set, mut made, mut skipped) = (0, 0, 0);
        for cell in cells {
            let Some(grid) = exterior_grid(&cell_key(cell)) else {
                return err(format!("{cell}: a region belongs to an exterior square, (x, y)"));
            };
            let key = format!("({}, {})", grid.0, grid.1);
            let found = match self.find("CELL", &key, None) {
                Some(f) => f,
                None if make => {
                    self.new_cell(&key, false)?;
                    made += 1;
                    match self.find("CELL", &key, None) {
                        Some(f) => f,
                        None => return err(format!("{key} was made but cannot be found")),
                    }
                }
                None => {
                    skipped += 1;
                    continue;
                }
            };
            self.queue_land_fields(&found, vec![("region", value.clone())])?;
            set += 1;
        }
        Ok(json!({"set": set, "made": made, "skipped": skipped}))
    }

    /// Fields into the patch (land's, a cell's region): onto the patch's own record, or as
    /// field values over the load order's (the winner as the base).
    fn queue_land_fields(&mut self, found: &Found, fields: Vec<(&str, Value)>) -> Result<(), EditError> {
        if found.new {
            let Some(mine) = self.queue.new_record(&found.record_type, &found.key).cloned() else {
                return err(format!("{} is no longer in the patch", found.key));
            };
            let mut record = mine.record.clone();
            for (path, value) in fields {
                record[path] = value;
            }
            self.queue.add_new_record(NewRecord { record, ..mine });
        } else {
            for (path, value) in fields {
                self.queue.add_field(&found.record_type, &found.key, Choice::Value { path: path.into(), value });
            }
            self.queue.set_base(&found.record_type, &found.key, found.winner());
        }
        self.save_journal();
        Ok(())
    }

    /// A cell's land textures set (the Land view's texture brush): `ids` the 16 x 16 texture
    /// cells' land texture ids, row by row from the south-west (`""` or `_land_default` for
    /// unpainted ground). A `VTEX` value only means something inside the plugin that wrote
    /// it, so every texture the cell wears gets a land texture record of the patch's own,
    /// copied from the load order's and numbered for the patch, and the cell's grid is
    /// written in that numbering. `{cell, textures}`.
    pub fn set_land_textures(&mut self, cell: &str, ids: &[String]) -> Result<Value, EditError> {
        let Some(grid) = exterior_grid(&cell_key(cell)) else {
            return err("land belongs to an exterior square: give (x, y)");
        };
        let key = format!("({}, {})", grid.0, grid.1);
        if ids.len() != 256 {
            return err(format!("a cell has 256 texture cells, not {}", ids.len()));
        }
        let Some(found) = self.find("LAND", &key, None) else {
            return err(format!("{key} has no land to paint: make land there first (New land)"));
        };
        // The patch's own land textures: what it numbers each id, the next free number.
        let mut numbered: HashMap<String, i64> = HashMap::new();
        let mut next = 0i64;
        for r in self.queue.new_records().filter(|r| r.record_type == "LandscapeTexture") {
            let i = r.record.get("index").and_then(Value::as_i64).unwrap_or(0);
            numbered.insert(r.key.to_lowercase(), i);
            next = next.max(i + 1);
        }
        let mut rows = Vec::with_capacity(256);
        let mut made = Vec::new();
        for id in ids {
            let id = id.trim();
            if id.is_empty() || id.eq_ignore_ascii_case("_land_default") {
                rows.push(0);
                continue;
            }
            let index = match numbered.get(&id.to_lowercase()) {
                Some(&i) => i,
                None => {
                    let Some(ltex) = self.find("LTEX", id, None) else {
                        return err(format!("no plugin has a land texture {id}"));
                    };
                    let mut record = ltex.record().clone();
                    record["index"] = json!(next);
                    let source = if ltex.new { String::new() } else { ltex.winner().to_string() };
                    made.push(NewRecord { record_type: "LandscapeTexture".into(), key: ltex.key.clone(), record, source, topic: String::new() });
                    numbered.insert(id.to_lowercase(), next);
                    next += 1;
                    next - 1
                }
            };
            rows.push(index + 1);
        }
        let mut lt = tes3::esp::Landscape { grid, ..Default::default() };
        let rows: Vec<landcore::grid::Value> = rows.iter().map(|&v| v as landcore::grid::Value).collect();
        landcore::emit::attach_texture_indices(&mut lt, &rows);
        let as_json = serde_json::to_value(tes3::esp::TES3Object::Landscape(lt)).map_err(|e| EditError(e.to_string()))?;
        let mut fields: Vec<(&str, Value)> = vec![("texture_indices", as_json["texture_indices"].clone())];
        let flags = found.record().get("landscape_flags").and_then(Value::as_str).unwrap_or("").to_string();
        if !flags.contains("USES_TEXTURES") {
            let with = if flags.trim().is_empty() { "USES_TEXTURES".to_string() } else { format!("{flags} | USES_TEXTURES") };
            fields.push(("landscape_flags", json!(with)));
        }
        let textures = made.len();
        self.batch(|ed| {
            for m in made {
                ed.queue.add_new_record(m);
            }
            if found.new {
                let Some(mine) = ed.queue.new_record(&found.record_type, &found.key).cloned() else {
                    return err(format!("{} is no longer in the patch", found.key));
                };
                let mut record = mine.record.clone();
                for (path, value) in fields {
                    record[path] = value;
                }
                ed.queue.add_new_record(NewRecord { record, ..mine });
            } else {
                for (path, value) in fields {
                    ed.queue.add_field(&found.record_type, &found.key, Choice::Value { path: path.into(), value });
                }
                ed.queue.set_base(&found.record_type, &found.key, found.winner());
            }
            ed.save_journal();
            Ok(())
        })?;
        Ok(json!({"cell": key, "textures": textures}))
    }

    /// A copy of a whole interior, a cell of the patch's own: the cell as the load order
    /// resolves it with the pool's changes on top, every reference placed anew, and its
    /// path grid. `{cell, refs, pathgrid}`.
    pub fn duplicate_cell(&mut self, cell: &str, new_name: &str) -> Result<Value, EditError> {
        let src = cell_key(cell);
        if exterior_grid(&src).is_some() {
            return err("only an interior can be copied: an exterior is a grid square");
        }
        let name = new_name.trim().to_string();
        if name.is_empty() {
            return err("the copy needs a name");
        }
        if name.chars().count() > MAX_CELL_NAME {
            return err(format!("a cell name is at most {MAX_CELL_NAME} characters"));
        }
        if self.find("CELL", &name, None).is_some() {
            return err(format!("there is a cell named {name} already"));
        }
        let holders = self.cell_plugins(&src, None);
        if holders.is_empty() {
            return err(format!("no plugin of this load order has the cell {src}"));
        }
        let want = src.to_lowercase();
        let versions: Vec<(String, Value)> = holders.iter().map(|p| (p.clone(), self.session.records(p, "CELL")[&want].clone())).collect();
        let (last_plugin, last) = versions.last().cloned().expect("a holder");
        let mut winner = last.clone();
        if let Value::Object(m) = &mut winner {
            m.shift_remove("references");
        }
        winner["name"] = json!(name);
        let key = record_key(&last).to_lowercase();
        let edits: HashMap<(String, i64), RefEdit> =
            self.queue.ref_edits().filter(|e| e.cell.to_lowercase() == key).map(|e| ((e.origin.to_lowercase(), e.refr_index), e.clone())).collect();
        // Every reference as the load order resolves it, the pool's changes on top.
        let mut seen: Vec<((String, i64), String)> = Vec::new();
        for (plugin, rec) in &versions {
            let masters = self.session.masters_of(plugin);
            for r in rec.get("references").and_then(Value::as_array).into_iter().flatten() {
                if !r.is_object() {
                    continue;
                }
                let mast = r.get("mast_index").and_then(Value::as_i64).unwrap_or(-1);
                let origin = match mast {
                    0 => Some(plugin.clone()),
                    m if m > 0 && (m as usize) <= masters.len() => Some(masters[m as usize - 1].clone()),
                    _ => None,
                };
                if let Some(o) = origin {
                    let k = (o.to_lowercase(), r.get("refr_index").and_then(Value::as_i64).unwrap_or(0));
                    match seen.iter_mut().find(|(x, _)| *x == k) {
                        Some(slot) => slot.1 = o,
                        None => seen.push((k, o)),
                    }
                }
            }
        }
        let mut placed: Vec<NewRef> = Vec::new();
        for ((low, refr), origin) in &seen {
            let Some((plugin, mut r)) = self.session.winning_reference(&versions, origin, *refr) else { continue };
            if let Some(edit) = edits.get(&(low.clone(), *refr)) {
                for (k, v) in edit.changes.iter() {
                    r[k] = v.clone();
                }
            }
            let truthy = |v: Option<&Value>| v.is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)) && v.as_str() != Some("") && v.as_f64() != Some(0.0));
            if truthy(r.get("deleted")) || !truthy(r.get("id")) || r.get("translation").is_none_or(Value::is_null) {
                continue;
            }
            let mut fields = crate::queue::Fields::default();
            for (k, v) in r.as_object().into_iter().flatten() {
                if (REF_FIELDS.contains(&k.as_str()) || k == "id") && k != "deleted" && k != "moved_cell" && !v.is_null() {
                    fields.insert(k.clone(), v.clone());
                }
            }
            placed.push(NewRef { cell: name.clone(), uid: token(), fields, base_plugin: plugin, plugins: Vec::new(), tag: String::new() });
        }
        let own: Vec<NewRef> = self.queue.new_refs().filter(|n| n.cell.to_lowercase() == key).cloned().collect();
        for n in own {
            placed.push(NewRef { cell: name.clone(), uid: token(), fields: n.fields, base_plugin: n.base_plugin, plugins: Vec::new(), tag: n.tag });
        }
        self.queue.add_new_record(NewRecord { record_type: "Cell".into(), key: name.clone(), record: winner, source: last_plugin, topic: String::new() });
        for n in &placed {
            self.queue.add_new_ref(n.clone());
        }
        // The path grid (as the patch would write it) comes too, under the new name.
        let grid = self.find("PGRD", &src, None);
        let has_grid = grid.is_some();
        if let Some(g) = grid {
            let mut rec = self.pathgrid_record(&g);
            rec["cell"] = json!(name);
            if let Some(Value::Object(d)) = rec.get_mut("data") {
                d.insert("grid".into(), json!([0, 0]));
            }
            let source = if g.new { String::new() } else { g.winner().to_string() };
            self.queue.add_new_record(NewRecord { record_type: "PathGrid".into(), key: name.clone(), record: rec, source, topic: String::new() });
        }
        self.save_journal();
        Ok(json!({"cell": name, "refs": placed.len(), "pathgrid": has_grid}))
    }

    // -- path grids ----------------------------------------------------------------------

    /// A cell as its path grid's key, and its exterior grid (`None` inside).
    pub fn pathgrid_key(cell: &str) -> (String, Option<(i32, i32)>) {
        match CellKey::parse(cell) {
            Some(k) => (k.pool_key(), k.grid()),
            None => (String::new(), None),
        }
    }

    /// The path grid as the patch would write it: the winner with the queued values.
    fn pathgrid_record(&self, found: &Found) -> Value {
        let mut record = found.record().clone();
        if !found.new {
            for c in self.queue.fields().get(&(found.record_type.clone(), found.key.clone())).into_iter().flatten() {
                if let Choice::Value { path, value } = c {
                    let _ = set_at(&mut record, path, value.clone());
                }
            }
        }
        record
    }

    /// A cell's path grid for the render window, as the patch would write it: `{key,
    /// grid, plugins, queued, new, granularity, points: [[x, y, z]], edges: [[a, b]]}`.
    pub fn pathgrid_view(&self, cell: &str) -> Value {
        let (key, grid) = Editor::pathgrid_key(cell);
        let grid_v = grid.map(|(x, y)| json!([x, y])).unwrap_or(Value::Null);
        let Some(found) = self.find("PGRD", &key, None) else {
            return json!({"key": key, "grid": grid_v, "points": [], "edges": [], "plugins": [], "queued": false, "new": false, "granularity": 1024});
        };
        let rec = self.pathgrid_record(&found);
        let points: Vec<Value> = rec.get("points").and_then(Value::as_array).cloned().unwrap_or_default();
        let int = |v: Option<&Value>| v.and_then(|x| x.as_f64().or_else(|| pyval::float_of(&pyval::str_of(x)))).map(|f| f as i64).unwrap_or(0);
        let counts: Vec<i64> = points.iter().map(|p| int(p.get("connection_count"))).collect();
        let expected = counts.iter().sum::<i64>().max(0) as usize;
        let targets = connections(rec.get("connections").unwrap_or(&Value::Null), expected);
        let mut edges = Vec::new();
        let mut at = 0usize;
        for (i, n) in counts.iter().enumerate() {
            let n = (*n).max(0) as usize;
            for t in targets.iter().skip(at).take(n) {
                if *t >= 0 && (*t as usize) < points.len() {
                    edges.push(json!([i, t]));
                }
            }
            at += n;
        }
        let granularity = rec.get("data").and_then(|d| d.get("granularity")).map(|g| int(Some(g))).filter(|g| *g != 0).unwrap_or(1024);
        let queued = found.new || self.queue.fields().get(&(found.record_type.clone(), found.key.clone())).is_some_and(|c| !c.is_empty());
        let locs: Vec<Value> = points
            .iter()
            .map(|p| {
                let l = p.get("location").and_then(Value::as_array).cloned().unwrap_or_else(|| vec![json!(0), json!(0), json!(0)]);
                Value::Array(l.iter().map(|v| json!(int(Some(v)))).collect())
            })
            .collect();
        json!({
            "key": key, "grid": grid_v, "points": locs, "edges": edges,
            "plugins": found.versions.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
            "queued": queued, "new": found.new, "granularity": granularity,
        })
    }

    /// Queues a cell's whole path grid: its points (`[x, y, z]` as the record holds them)
    /// and links (`[a, b]` one way each). A cell with none gets one, the patch's own.
    pub fn set_pathgrid(&mut self, cell: &str, points: &[Value], edges: &[Value]) -> Result<Value, EditError> {
        let (key, grid) = Editor::pathgrid_key(cell);
        if points.len() > 65535 {
            return err("a path grid holds at most 65535 points");
        }
        let mut locs: Vec<[i64; 3]> = Vec::new();
        for (i, p) in points.iter().enumerate() {
            let xs: Option<Vec<f64>> = p.as_array().filter(|a| a.len() == 3).and_then(|a| a.iter().map(float_value).collect());
            match xs.filter(|x| x.iter().all(|v| v.is_finite())) {
                Some(x) => locs.push([x[0].round_ties_even() as i64, x[1].round_ties_even() as i64, x[2].round_ties_even() as i64]),
                None => return err(format!("point {i} is not [x, y, z]")),
            }
        }
        let mut links: Vec<Vec<i64>> = vec![Vec::new(); locs.len()];
        for e in edges {
            let end = |i: usize| e.get(i).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()).map(|n| n as f64))).map(|f| f as i64);
            let (a, b) = (end(0).unwrap_or(-1), end(1).unwrap_or(-1));
            if !((0..locs.len() as i64).contains(&a) && (0..locs.len() as i64).contains(&b)) || a == b {
                return err(format!("the link {a} -> {b} does not join two points"));
            }
            if !links[a as usize].contains(&b) {
                links[a as usize].push(b);
            }
        }
        let found = self.find("PGRD", &key, None);
        let old: Vec<Value> = found.as_ref().and_then(|f| self.pathgrid_record(f).get("points").and_then(Value::as_array).cloned()).unwrap_or_default();
        let auto_of = |p: &Value| p.get("auto_generated").and_then(Value::as_i64).unwrap_or(0);
        let auto = match old.first() {
            Some(p) if p.is_object() => auto_of(p),
            _ => 1,
        };
        let new_points: Vec<Value> = locs
            .iter()
            .enumerate()
            .map(|(i, loc)| {
                let a = old.get(i).filter(|p| p.is_object()).map(auto_of).unwrap_or(auto);
                json!({"location": loc, "auto_generated": a, "connection_count": links[i].len()})
            })
            .collect();
        let conns: Vec<i64> = links
            .iter()
            .flat_map(|l| {
                let mut s = l.clone();
                s.sort();
                s
            })
            .collect();
        let Some(found) = found else {
            let cell_found = self.find("CELL", &key, None);
            let mut record = json!({
                "type": "PathGrid", "flags": "", "cell": if grid.is_some() { String::new() } else { key.clone() },
                "data": {"grid": grid.map(|(x, y)| json!([x, y])).unwrap_or(json!([0, 0])), "granularity": 1024, "point_count": locs.len()},
                "points": new_points, "connections": conns,
            });
            if let (Some(c), Some(_)) = (&cell_found, grid) {
                record["cell"] = json!(c.record().get("name").and_then(Value::as_str).unwrap_or(""));
            }
            let source = cell_found.filter(|c| !c.new).map(|c| c.winner().to_string()).unwrap_or_default();
            self.queue.add_new_record(NewRecord { record_type: "PathGrid".into(), key: key.clone(), record, source, topic: String::new() });
            self.save_journal();
            return Ok(self.pathgrid_view(cell));
        };
        if found.new {
            let Some(made) = self.queue.new_record(&found.record_type, &found.key).cloned() else {
                return err(format!("{} is no longer in the patch", found.key));
            };
            let mut record = made.record.clone();
            record["points"] = json!(new_points);
            record["connections"] = json!(conns);
            if !record.get("data").is_some_and(Value::is_object) {
                record["data"] = json!({});
            }
            record["data"]["point_count"] = json!(locs.len());
            self.queue.add_new_record(NewRecord { record, ..made });
            self.save_journal();
            return Ok(self.pathgrid_view(cell));
        }
        for (path, value) in [("points", json!(new_points)), ("connections", json!(conns)), ("data.point_count", json!(locs.len()))] {
            self.queue.add_field(&found.record_type, &found.key, Choice::Value { path: path.into(), value });
        }
        self.queue.set_base(&found.record_type, &found.key, found.winner());
        self.save_journal();
        Ok(self.pathgrid_view(cell))
    }

    /// Drops a cell's queued path grid (a new one with it).
    pub fn revert_pathgrid(&mut self, cell: &str) -> Value {
        let (key, _) = Editor::pathgrid_key(cell);
        if let Some(found) = self.find("PGRD", &key, None) {
            if found.new {
                self.queue.remove_new_record(&found.record_type, &found.key);
                self.save_journal();
            } else {
                let _ = self.revert(&found, None);
            }
        }
        self.pathgrid_view(cell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_links_read_as_the_crate_writes_them() {
        let grid = tes3::esp::TES3Object::PathGrid(tes3::esp::PathGrid { connections: vec![1, 0, 2, 1], ..Default::default() });
        let v: Value = serde_json::from_str(&serde_json::to_string(&grid).unwrap()).unwrap();
        assert_eq!(connections(&v["connections"], 4), vec![1, 0, 2, 1]);
        // As a list (the patch's own path grids), and a count prefix only taken off when
        // it reads as one.
        assert_eq!(connections(&json!([3, 1]), 2), vec![3, 1]);
        let land = flat_land((0, 0), 0.0, 0).unwrap();
        let back: Result<tes3::esp::TES3Object, _> = serde_json::from_value(land);
        assert!(back.is_ok(), "flat land is a record the crate reads: {:?}", back.err());
    }

    /// The Land view's write: heights in, a LAND record the build can write out, with the
    /// normals and world map derived from them - for the load order's land and the patch's own.
    #[test]
    fn flat_land_is_at_its_height_as_the_game_reads_it() {
        for h in [DEFAULT_LAND_HEIGHT, 0.0, 256.0] {
            let rec = flat_land((4, 4), h, 0).unwrap();
            let lt: tes3::esp::TES3Object = serde_json::from_value(rec).unwrap();
            let tes3::esp::TES3Object::Landscape(l) = lt else { panic!() };
            let raw: Vec<u8> = l.vertex_heights.data.iter().flatten().map(|&d| d as u8).collect();
            let back = landcore::heights::decode_heights(&raw, l.vertex_heights.offset as f64).unwrap();
            assert!(back.iter().all(|&v| v as f64 == h), "{h}: {:?}", &back[..3]);
        }
    }

    #[test]
    fn shaped_heights_queue_as_landcore_encodes_them() {
        use crate::queue::PatchQueue;
        use crate::session::Session;
        let land = flat_land((2, -3), 256.0, 0).unwrap();
        let header = json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esm", "author": "", "description": "", "num_objects": 1, "masters": []});
        let session = Session::with_reader(
            vec![("Base.esm".into(), std::path::PathBuf::from("Base.esm"))],
            Box::new(move |_, tag| Ok(match tag { "TES3" => vec![header.clone()], "LAND" => vec![land.clone()], _ => Vec::new() })),
        );
        let mut ed = Editor::new(session, PatchQueue::new(), None);
        // A hill in the middle of the cell.
        let heights: Vec<f64> = (0..65 * 65).map(|i| { let (x, y) = ((i % 65) as f64 - 32.0, (i / 65) as f64 - 32.0); 256.0 + (1000.0 - (x * x + y * y) * 4.0).max(0.0) }).collect();
        let r = ed.set_land("2, -3", &heights).unwrap();
        assert_eq!((r["cell"].as_str(), r["clamped"].as_u64()), (Some("(2, -3)"), Some(0)));
        let queued = ed.queue.fields().get(&("Landscape".to_string(), "(2, -3)".to_string())).cloned().unwrap();
        let paths: Vec<&str> = queued.iter().map(Choice::path).collect();
        assert_eq!(paths, ["vertex_heights", "vertex_normals", "world_map_data"]);
        // The queued heights read back as written (to the format's 8-unit step).
        let mut rec = flat_land((2, -3), 0.0, 0).unwrap();
        for c in &queued {
            let Choice::Value { path, value } = c else { panic!() };
            rec[path.as_str()] = value.clone();
        }
        let lt: tes3::esp::TES3Object = serde_json::from_value(rec).unwrap();
        let tes3::esp::TES3Object::Landscape(l) = lt else { panic!() };
        let raw: Vec<u8> = l.vertex_heights.data.iter().flatten().map(|&d| d as u8).collect();
        let back = landcore::heights::decode_heights(&raw, l.vertex_heights.offset as f64).unwrap();
        for (a, b) in back.iter().zip(&heights) {
            assert!((*a as f64 - b).abs() < 8.0, "{a} vs {b}");
        }
        // The Land view reads it back to draw over the load order's.
        let pending = ed.land_pending();
        let p = &pending[0];
        assert_eq!(p["cell"], "(2, -3)");
        assert!(p.get("colors").is_none() && p.get("ids").is_none());
        assert!((p["heights"][32 * 65 + 32].as_f64().unwrap() - heights[32 * 65 + 32]).abs() < 8.0);
        // No land, no shaping; and the wrong number of heights is refused.
        assert!(ed.set_land("5, 5", &heights).is_err());
        assert!(ed.set_land("2, -3", &heights[..100]).is_err());
    }

    /// The texture brush's write: ids in, the patch's own land textures numbered from zero,
    /// and the cell's grid in that numbering (a `VTEX` value means nothing outside its plugin).
    #[test]
    fn painted_textures_get_the_patch_s_own_numbering() {
        use crate::queue::PatchQueue;
        use crate::session::Session;
        let land = flat_land((0, 0), 0.0, 0).unwrap();
        let ltex = json!({"type": "LandscapeTexture", "flags": "", "id": "Grass", "index": 7, "file_name": "grass.dds"});
        let header = json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esm", "author": "", "description": "", "num_objects": 2, "masters": []});
        let session = Session::with_reader(
            vec![("Base.esm".into(), std::path::PathBuf::from("Base.esm"))],
            Box::new(move |_, tag| Ok(match tag { "TES3" => vec![header.clone()], "LAND" => vec![land.clone()], "LTEX" => vec![ltex.clone()], _ => Vec::new() })),
        );
        let mut ed = Editor::new(session, PatchQueue::new(), None);
        let mut ids = vec![String::new(); 256];
        ids[0] = "grass".into();
        ids[17] = "Grass".into();
        ids[255] = "_land_default".into();
        assert_eq!(ed.set_land_textures("(0, 0)", &ids).unwrap()["textures"], 1);
        let made = ed.queue.new_record("LandscapeTexture", "Grass").cloned().unwrap();
        assert_eq!((made.record["index"].as_i64(), made.source.as_str()), (Some(0), "Base.esm"));
        // A second cell reuses the patch's number for the same texture.
        assert_eq!(ed.set_land_textures("(0, 0)", &ids).unwrap()["textures"], 0);
        let queued = ed.queue.fields().get(&("Landscape".to_string(), "(0, 0)".to_string())).cloned().unwrap();
        let mut rec = flat_land((0, 0), 0.0, 0).unwrap();
        for c in &queued {
            let Choice::Value { path, value } = c else { panic!() };
            rec[path.as_str()] = value.clone();
        }
        let tes3::esp::TES3Object::Landscape(l) = serde_json::from_value(rec).unwrap() else { panic!() };
        let raw: Vec<u8> = l.texture_indices.data.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
        let rows = landcore::layers::decode_texture_indices(&raw);
        assert_eq!((rows[0], rows[17], rows[1], rows[255]), (1, 1, 0, 0));
        let pending = ed.land_pending();
        assert_eq!((pending[0]["ids"][0].as_str(), pending[0]["ids"][1].as_str()), (Some("Grass"), Some("_land_default")));
        assert!(ed.set_land_textures("(0, 0)", &vec!["Nothing".to_string(); 256]).is_err());
    }

    /// The color brush's write: the cell's vertex colors, the rest of its record kept.
    #[test]
    fn painted_colors_queue_as_vertex_colors() {
        use crate::queue::PatchQueue;
        use crate::session::Session;
        let land = flat_land((1, 1), 0.0, 0).unwrap();
        let header = json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esm", "author": "", "description": "", "num_objects": 1, "masters": []});
        let session = Session::with_reader(
            vec![("Base.esm".into(), std::path::PathBuf::from("Base.esm"))],
            Box::new(move |_, tag| Ok(match tag { "TES3" => vec![header.clone()], "LAND" => vec![land.clone()], _ => Vec::new() })),
        );
        let mut ed = Editor::new(session, PatchQueue::new(), None);
        let colors: Vec<f64> = (0..65 * 65 * 3).map(|i| [200.0, 120.0, 300.0][i % 3]).collect();
        ed.set_land_colors("(1, 1)", &colors).unwrap();
        let queued = ed.queue.fields().get(&("Landscape".to_string(), "(1, 1)".to_string())).cloned().unwrap();
        let mut rec = flat_land((1, 1), 0.0, 0).unwrap();
        for c in &queued {
            let Choice::Value { path, value } = c else { panic!() };
            rec[path.as_str()] = value.clone();
        }
        let tes3::esp::TES3Object::Landscape(l) = serde_json::from_value(rec).unwrap() else { panic!() };
        assert_eq!(l.vertex_colors.data[3][7], [200, 120, 255]);
        assert!(ed.set_land_colors("(1, 1)", &colors[..30]).is_err());
    }

    /// The region map (TODO, "The world"): an exterior's region set as a field of its cell,
    /// whether the load order has the cell or the patch made it.
    #[test]
    fn a_region_painted_onto_cells_queues_as_their_region_field() {
        use crate::queue::PatchQueue;
        use crate::session::Session;
        let header = json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esm", "author": "", "description": "", "num_objects": 1, "masters": []});
        let cell = json!({"type": "Cell", "flags": "", "name": "", "data": {"flags": "", "grid": [4, 5]}, "references": []});
        let session = Session::with_reader(
            vec![("Base.esm".into(), std::path::PathBuf::from("Base.esm"))],
            Box::new(move |_, tag| Ok(match tag {
                "TES3" => vec![header.clone()],
                "CELL" => vec![cell.clone()],
                "REGN" => vec![json!({"type": "Region", "flags": "", "id": "Bitter Coast Region", "name": "Bitter Coast", "sleep_creature": "", "map_color": [30, 80, 40, 0], "sounds": [],
                    "weather_chances": {"clear": 100, "cloudy": 0, "foggy": 0, "overcast": 0, "rain": 0, "thunder": 0, "ash": 0, "blight": 0, "snow": 0, "blizzard": 0}})],
                _ => Vec::new(),
            })),
        );
        let mut ed = Editor::new(session, PatchQueue::new(), None);
        // No such region: refused.
        assert!(ed.set_cell_regions(&["(4, 5)".into()], "Nowhere Region", false).is_err());
        // The load order's cell, and an empty square: left alone unless `make`.
        let r = ed.set_cell_regions(&["(4, 5)".into(), "(9, 9)".into()], "", false).unwrap();
        assert_eq!((r["set"].as_u64(), r["skipped"].as_u64()), (Some(1), Some(1)));
        let queued = ed.queue.fields().get(&("Cell".to_string(), "(4, 5)".to_string())).cloned().unwrap();
        assert!(queued.iter().any(|c| matches!(c, Choice::Value { path, value } if path == "region" && value.is_null())));
        // Made, then painted.
        let r = ed.set_cell_regions(&["(9, 9)".into()], "", true).unwrap();
        assert_eq!(r["made"].as_u64(), Some(1));
        assert!(ed.queue.new_record("Cell", "(9, 9)").is_some());
        // A real region, on both: the id as the load order spells it.
        ed.set_cell_regions(&["(4, 5)".into(), "(9, 9)".into()], "bitter coast region", false).unwrap();
        assert_eq!(ed.queue.new_record("Cell", "(9, 9)").unwrap().record["region"], json!("Bitter Coast Region"));
        let queued = ed.queue.fields().get(&("Cell".to_string(), "(4, 5)".to_string())).cloned().unwrap();
        assert!(queued.iter().any(|c| matches!(c, Choice::Value { path, value } if path == "region" && value == "Bitter Coast Region")));
    }
}
