//! A plugin's records read by greatness7's `tes3::esp`, one record at a time, into
//! what `parse_plugin` keeps.
//!
//! Each record is loaded on its own, so a record the crate refuses (malformed, or a tag
//! it does not model) costs only that record: `parse_plugin` then reads it with the
//! engine's own subrecord walk. Two kinds always use that walk, because the crate's
//! types cannot say whether a subrecord was present:
//!
//! * `LAND`: a missing `VNML` or `VCLR` reads as zeros, where the engine needs to know
//!   it is missing (it derives normals, and draws no vertex colours).
//! * `CREA`: a missing `NPDT` reads as health 0, which the engine would take for a
//!   creature that spawns dead.

use tes3::esp::{ObjectInfo, TES3Object};

use super::{is_animated_class, ActorDef, LightDef, ModelDef, PluginData, RawCell};
use crate::esp::{is_interior, Ambi, CellRef, DoorDest, RefNum, Tag};

/// Whether `parse_plugin` should offer a record of this tag to the crate.
pub(super) fn by_crate(tag: Tag) -> bool {
    !matches!(&tag, b"LAND" | b"CREA")
}

/// Loads one whole record (its 16-byte header and body). `None` when the crate refuses
/// it or panics on it.
pub(super) fn load(record: &[u8]) -> Option<TES3Object> {
    std::panic::catch_unwind(|| bytes_io::Reader::new(record).load::<TES3Object>().ok()).ok().flatten()
}

/// Adds what the world keeps of `obj` to `d`. `flags` is the record header's flags word.
pub(super) fn take(d: &mut PluginData, obj: TES3Object, tag: Tag, flags: u32, plugin_idx: i32, masters: &[i32]) {
    macro_rules! model {
        ($r:expr, $script:expr) => {{
            let r = $r;
            if !r.id.is_empty() {
                d.models.push(ModelDef {
                    id: r.id.clone(),
                    mesh: r.mesh.clone(),
                    deleted: r.deleted(),
                    animated_class: is_animated_class(tag),
                    scripted: !$script.is_empty(),
                });
            }
        }};
    }
    match obj {
        TES3Object::Header(h) => d.masters.extend(h.masters.into_iter().map(|(name, _size)| name)),
        TES3Object::LandscapeTexture(t) => {
            if !t.deleted() && !t.id.is_empty() {
                d.ltex.push((t.index, t.id, t.file_name));
            }
        }
        TES3Object::Region(r) => {
            if !r.deleted() && !r.id.is_empty() {
                d.regions.push((r.id, r.name));
            }
        }
        TES3Object::Cell(c) => {
            let flags = c.data.flags.bits();
            // The crate reads `INTV` and `WHGT` into one field; a level counts only for
            // an interior (see `esp::parse_cell_header`).
            let water = if is_interior(flags) { c.water_height } else { None };
            let ambi = c.atmosphere_data.as_ref().map(|a| Ambi {
                ambient: [a.ambient_color[0], a.ambient_color[1], a.ambient_color[2]],
                sunlight: [a.sunlight_color[0], a.sunlight_color[1], a.sunlight_color[2]],
                fog: [a.fog_color[0], a.fog_color[1], a.fog_color[2]],
                fog_density: a.fog_density,
            });
            /* The crate keeps a cell's references by (master, index), which is also how
               the merge keys them; sorted, so a plugin always reads the same way. */
            let mut keyed: Vec<_> = c.references.into_iter().collect();
            keyed.sort_unstable_by_key(|(k, _)| *k);
            let refs = keyed
                .into_iter()
                .map(|((mast, refr), r)| CellRef {
                    num: RefNum::resolve((mast << 24) | (refr & 0x00ff_ffff), plugin_idx, masters),
                    id: r.id,
                    pos: r.translation,
                    rot: r.rotation,
                    scale: r.scale.unwrap_or(1.0),
                    deleted: r.deleted.unwrap_or(false),
                    moved_to: r.moved_cell,
                    door: r.destination.map(|t| DoorDest { pos: t.translation, rot: t.rotation, cell: t.cell }),
                    ..Default::default()
                })
                .collect();
            d.cells.push(RawCell {
                name: c.name,
                flags,
                grid: c.data.grid,
                region: c.region.unwrap_or_default(),
                water,
                ambi,
                refs,
            });
        }
        TES3Object::Npc(n) => {
            if !n.id.is_empty() {
                d.actors.push(ActorDef {
                    id: n.id.clone(),
                    model: n.mesh.clone(),
                    twin: None,
                    creature: false,
                    health: n.data.stats.as_ref().map(|s| s.health as i16 as i32),
                    persistent: flags & super::REC_PERSISTENT != 0,
                    deleted: n.deleted(),
                });
            }
        }
        TES3Object::Light(l) => {
            if !l.id.is_empty() {
                let c = l.data.color;
                d.lights.push(LightDef {
                    id: l.id.clone(),
                    radius: l.data.radius as i32,
                    colour: [c[0], c[1], c[2]],
                    flags: l.data.flags.bits() as i32,
                    deleted: l.deleted(),
                });
            }
            model!(&l, l.script)
        }
        TES3Object::Static(r) => model!(&r, ""),
        TES3Object::Activator(r) => model!(&r, r.script),
        TES3Object::Door(r) => model!(&r, r.script),
        TES3Object::Container(r) => model!(&r, r.script),
        TES3Object::MiscItem(r) => model!(&r, r.script),
        TES3Object::Weapon(r) => model!(&r, r.script),
        TES3Object::Armor(r) => model!(&r, r.script),
        TES3Object::Clothing(r) => model!(&r, r.script),
        TES3Object::Book(r) => model!(&r, r.script),
        TES3Object::Alchemy(r) => model!(&r, r.script),
        TES3Object::Apparatus(r) => model!(&r, r.script),
        TES3Object::Ingredient(r) => model!(&r, r.script),
        TES3Object::Lockpick(r) => model!(&r, r.script),
        TES3Object::Probe(r) => model!(&r, r.script),
        TES3Object::RepairItem(r) => model!(&r, r.script),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::{parse_plugin_with, PluginData};

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    fn rec(tag: &[u8; 4], subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.concat();
        let mut v = tag.to_vec();
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(&[0; 8]);
        v.extend_from_slice(&body);
        v
    }

    fn floats(f: &[f32]) -> Vec<u8> {
        f.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A header, a static, a light, and an interior with a placed rock, a deleted
    /// reference and a door.
    fn plugin() -> Vec<u8> {
        let mut hedr = Vec::new();
        hedr.extend_from_slice(&1.3f32.to_le_bytes());
        hedr.extend_from_slice(&0u32.to_le_bytes());
        hedr.extend_from_slice(&[0; 32 + 256]);
        hedr.extend_from_slice(&4u32.to_le_bytes());
        let mut lhdt = floats(&[1.0]);
        for v in [5i32, 100, 256] {
            lhdt.extend_from_slice(&v.to_le_bytes());
        }
        lhdt.extend_from_slice(&[255, 128, 0, 0]);
        lhdt.extend_from_slice(&0x24i32.to_le_bytes());
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend_from_slice(&[0; 8]);
        let place = floats(&[1.0, 2.0, 3.0, 0.0, 0.0, 1.5]);
        [
            rec(b"TES3", &[sub(b"HEDR", &hedr)]),
            rec(b"STAT", &[sub(b"NAME", b"rock\0"), sub(b"MODL", b"r\\rock.nif\0")]),
            rec(b"LIGH", &[sub(b"NAME", b"lamp\0"), sub(b"MODL", b"l\\lamp.nif\0"), sub(b"LHDT", &lhdt)]),
            rec(
                b"CELL",
                &[
                    sub(b"NAME", b"Test Room\0"),
                    sub(b"DATA", &data),
                    sub(b"INTV", &(-760i32).to_le_bytes()),
                    sub(b"FRMR", &1u32.to_le_bytes()),
                    sub(b"NAME", b"rock\0"),
                    sub(b"XSCL", &floats(&[1.5])),
                    sub(b"DATA", &place),
                    sub(b"FRMR", &2u32.to_le_bytes()),
                    sub(b"NAME", b"rock\0"),
                    sub(b"DELE", &0u32.to_le_bytes()),
                    sub(b"DATA", &place),
                    sub(b"FRMR", &3u32.to_le_bytes()),
                    sub(b"NAME", b"door\0"),
                    sub(b"DODT", &floats(&[4.0, 5.0, 6.0, 0.0, 0.0, 0.0])),
                    sub(b"DNAM", b"Elsewhere\0"),
                    sub(b"DATA", &place),
                ],
            ),
        ]
        .concat()
    }

    fn summary(d: &PluginData) -> String {
        let cells: Vec<String> = d
            .cells
            .iter()
            .map(|c| format!("{:?} {} {:?} {:?} {:?}", c.name, c.flags, c.grid, c.water, c.refs))
            .collect();
        format!("{:?} {:?} {:?} {:?} {}", d.models, d.lights, d.actors, cells, d.records)
    }

    #[test]
    fn the_crate_reads_what_the_subrecord_walk_reads() {
        let b = plugin();
        let by_crate = parse_plugin_with(&b, 3, &[], false, true);
        let own = parse_plugin_with(&b, 3, &[], false, false);
        assert_eq!(summary(&by_crate), summary(&own));
        let room = &by_crate.cells[0];
        assert_eq!(room.water, Some(-760.0));
        assert_eq!(room.refs.len(), 3);
        assert!(room.refs[1].deleted);
        assert_eq!(room.refs[0].scale, 1.5);
        assert_eq!(room.refs[2].door.as_ref().map(|d| d.cell.as_str()), Some("Elsewhere"));
        assert_eq!(by_crate.lights[0].radius, 256);
        assert_eq!(by_crate.lights[0].colour, [255, 128, 0]);
    }

    #[test]
    fn a_record_the_crate_refuses_is_read_by_the_walk() {
        // A STAT with a subrecord the crate does not expect: refused there, kept here.
        let mut b = plugin();
        b.extend(rec(b"STAT", &[sub(b"NAME", b"odd\0"), sub(b"MODL", b"odd.nif\0"), sub(b"ZZZZ", b"?")]));
        let d = parse_plugin_with(&b, 0, &[], false, true);
        assert!(d.models.iter().any(|m| m.id == "odd" && m.mesh == "odd.nif"));
    }
}
