//! Records: G7's `tes3::esp` types into Gardenfell's `tes3::types`.
//!
//! Covers the kinds Gardenfell's world types (`world::keeps`): the header, cells,
//! landscape, landscape textures, regions, the sixteen model kinds, creatures, NPCs and
//! leveled creature lists. Any other kind is typed by Robin's own reader, as the world
//! never types those (it reads them raw).
//!
//! The two models agree field for field (Robin's are G7's layouts); the conversion is
//! flags and enums to their numbers, and three things G7's types do not keep, read from
//! the raw record instead:
//! - whether an optional subrecord was there at all (land data, AI data, Bloodmoon's
//!   weather chances): G7 fills a default where Robin has `None`;
//! - a cell's references in file order, and which are temporary: G7 keys them by index;
//! - an actor's `FLAG` bits G7 does not know (Robin keeps the whole word);
//! - an `MVRF` naming another reference than the `FRMR` after it, which G7 drops and
//!   Robin keeps (`moved_index`). Such a cell, and one that repeats a reference's key
//!   (G7 keeps the last only), is typed by Robin's reader instead.

use gardenfell_bytes::codepage::{self, CodePage};
use gardenfell_tes3::tes3::raw::Record as RawRecord;
use gardenfell_tes3::tes3::types as gf;
use tes3::esp::{self as g7, TES3Object};

/// The engine's record hook: G7's crate types the record, this converts it.
pub fn type_record(rec: &RawRecord) -> Result<Option<gf::Object>, String> {
    if !covered(&rec.tag) {
        return gf::Object::from_raw(rec).map_err(|e| e.to_string());
    }
    let bytes = record_bytes(rec);
    let obj = std::panic::catch_unwind(|| bytes_io::Reader::new(&bytes).load::<TES3Object>().ok())
        .ok()
        .flatten()
        .ok_or_else(|| format!("tes3 could not read this {}", rec.tag_str()))?;
    convert(obj, rec)
}

/// The kinds this converts (Gardenfell's world types these and no others).
fn covered(tag: &[u8; 4]) -> bool {
    matches!(
        tag,
        b"TES3"
            | b"CELL"
            | b"LTEX"
            | b"REGN"
            | b"LAND"
            | b"STAT"
            | b"ACTI"
            | b"DOOR"
            | b"CONT"
            | b"LIGH"
            | b"MISC"
            | b"WEAP"
            | b"ARMO"
            | b"CLOT"
            | b"BOOK"
            | b"ALCH"
            | b"APPA"
            | b"INGR"
            | b"LOCK"
            | b"PROB"
            | b"REPA"
            | b"CREA"
            | b"NPC_"
            | b"LEVC"
    )
}

/// The record as the file holds it: header, then each subrecord.
fn record_bytes(rec: &RawRecord) -> Vec<u8> {
    let body: usize = rec.subs.iter().map(|s| 8 + s.data.len()).sum();
    let mut out = Vec::with_capacity(16 + body);
    out.extend_from_slice(&rec.tag);
    out.extend_from_slice(&(body as u32).to_le_bytes());
    out.extend_from_slice(&rec.unknown.to_le_bytes());
    out.extend_from_slice(&rec.flags.to_le_bytes());
    for s in &rec.subs {
        out.extend_from_slice(&s.tag);
        out.extend_from_slice(&(s.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&s.data);
    }
    out
}

/// G7's text in the code page the engine reads in: G7 always decodes Windows-1252, which
/// gives every byte back, so the bytes are recovered and decoded again.
fn t(s: &str) -> String {
    let page = codepage::current();
    if page == CodePage::Win1252 || s.is_ascii() {
        return s.to_string();
    }
    match CodePage::Win1252.encode(s) {
        Ok(b) => page.decode(&b),
        Err(_) => s.to_string(),
    }
}

fn ot(s: &Option<String>) -> Option<String> {
    s.as_deref().map(t)
}

fn convert(obj: TES3Object, rec: &RawRecord) -> Result<Option<gf::Object>, String> {
    use gf::Object as O;
    let has = |tag: &[u8; 4]| rec.has(tag);
    Ok(Some(match obj {
        TES3Object::Header(h) => O::Header(gf::Header {
            flags: h.flags.bits(),
            version: h.version,
            file_type: h.file_type as u32,
            author: t(&h.author.0),
            description: t(&h.description.0),
            num_objects: h.num_objects,
            masters: h.masters.iter().map(|(n, s)| (t(n), *s)).collect(),
        }),
        TES3Object::Cell(c) => match cell(c, rec) {
            Some(c) => O::Cell(c),
            // A cell G7's map cannot hold as the file has it: Robin's reader types it.
            None => return gf::Object::from_raw(rec).map_err(|e| e.to_string()),
        },
        TES3Object::Landscape(l) => O::Landscape(gf::Landscape {
            flags: l.flags.bits(),
            grid: l.grid,
            landscape_flags: l.landscape_flags.bits(),
            vertex_normals: has(b"VNML").then_some(gf::VertexNormals {
                data: l.vertex_normals.data,
            }),
            vertex_heights: has(b"VHGT").then_some(gf::VertexHeights {
                offset: l.vertex_heights.offset,
                data: l.vertex_heights.data,
            }),
            world_map_data: has(b"WNAM").then_some(gf::WorldMapData {
                data: l.world_map_data.data,
            }),
            vertex_colors: has(b"VCLR").then_some(gf::VertexColors {
                data: l.vertex_colors.data,
            }),
            texture_indices: has(b"VTEX").then_some(gf::TextureIndices {
                data: l.texture_indices.data,
            }),
        }),
        TES3Object::LandscapeTexture(l) => O::LandscapeTexture(gf::LandscapeTexture {
            flags: l.flags.bits(),
            id: t(&l.id),
            index: l.index,
            file_name: t(&l.file_name),
        }),
        TES3Object::Region(r) => {
            let bloodmoon = rec
                .sub(b"WEAT")
                .map(|s| s.data.len() >= 10)
                .unwrap_or(false);
            let w = &r.weather_chances;
            O::Region(gf::Region {
                flags: r.flags.bits(),
                id: t(&r.id),
                name: t(&r.name),
                weather_chances: gf::WeatherChances {
                    clear: w.clear,
                    cloudy: w.cloudy,
                    foggy: w.foggy,
                    overcast: w.overcast,
                    rain: w.rain,
                    thunder: w.thunder,
                    ash: w.ash,
                    blight: w.blight,
                    snow: bloodmoon.then_some(w.snow),
                    blizzard: bloodmoon.then_some(w.blizzard),
                },
                sleep_creature: t(&r.sleep_creature),
                map_color: r.map_color,
                sounds: r.sounds.iter().map(|(s, c)| (t(&s.0), *c)).collect(),
            })
        }
        TES3Object::Static(s) => O::Static(gf::Static {
            flags: s.flags.bits(),
            id: t(&s.id),
            mesh: t(&s.mesh),
        }),
        TES3Object::Activator(a) => O::Activator(gf::Activator {
            flags: a.flags.bits(),
            id: t(&a.id),
            name: t(&a.name),
            script: t(&a.script),
            mesh: t(&a.mesh),
        }),
        TES3Object::Door(d) => O::Door(gf::Door {
            flags: d.flags.bits(),
            id: t(&d.id),
            name: t(&d.name),
            script: t(&d.script),
            mesh: t(&d.mesh),
            open_sound: t(&d.open_sound),
            close_sound: t(&d.close_sound),
        }),
        TES3Object::Container(c) => O::Container(gf::Container {
            flags: c.flags.bits(),
            id: t(&c.id),
            name: t(&c.name),
            script: t(&c.script),
            mesh: t(&c.mesh),
            encumbrance: c.encumbrance,
            container_flags: c.container_flags.bits(),
            inventory: c.inventory.iter().map(|(n, id)| (*n, t(&id.0))).collect(),
        }),
        TES3Object::Light(l) => O::Light(gf::Light {
            flags: l.flags.bits(),
            id: t(&l.id),
            name: t(&l.name),
            script: t(&l.script),
            mesh: t(&l.mesh),
            icon: t(&l.icon),
            sound: t(&l.sound),
            data: gf::LightData {
                weight: l.data.weight,
                value: l.data.value,
                time: l.data.time,
                radius: l.data.radius,
                color: l.data.color,
                flags: l.data.flags.bits(),
            },
        }),
        TES3Object::MiscItem(m) => O::MiscItem(gf::MiscItem {
            flags: m.flags.bits(),
            id: t(&m.id),
            name: t(&m.name),
            script: t(&m.script),
            mesh: t(&m.mesh),
            icon: t(&m.icon),
            data: gf::MiscItemData {
                weight: m.data.weight,
                value: m.data.value,
                flags: m.data.flags.bits(),
            },
        }),
        TES3Object::Weapon(w) => O::Weapon(gf::Weapon {
            flags: w.flags.bits(),
            id: t(&w.id),
            name: t(&w.name),
            script: t(&w.script),
            mesh: t(&w.mesh),
            icon: t(&w.icon),
            enchanting: t(&w.enchanting),
            data: gf::WeaponData {
                weight: w.data.weight,
                value: w.data.value,
                weapon_type: w.data.weapon_type as u16,
                health: w.data.health,
                speed: w.data.speed,
                reach: w.data.reach,
                enchantment: w.data.enchantment,
                chop_min: w.data.chop_min,
                chop_max: w.data.chop_max,
                slash_min: w.data.slash_min,
                slash_max: w.data.slash_max,
                thrust_min: w.data.thrust_min,
                thrust_max: w.data.thrust_max,
                flags: w.data.flags.bits(),
            },
        }),
        TES3Object::Armor(a) => O::Armor(gf::Armor {
            flags: a.flags.bits(),
            id: t(&a.id),
            name: t(&a.name),
            script: t(&a.script),
            mesh: t(&a.mesh),
            icon: t(&a.icon),
            enchanting: t(&a.enchanting),
            biped_objects: a.biped_objects.iter().map(biped).collect(),
            data: gf::ArmorData {
                armor_type: a.data.armor_type as u32,
                weight: a.data.weight,
                value: a.data.value,
                health: a.data.health,
                enchantment: a.data.enchantment,
                armor_rating: a.data.armor_rating,
            },
        }),
        TES3Object::Clothing(c) => O::Clothing(gf::Clothing {
            flags: c.flags.bits(),
            id: t(&c.id),
            name: t(&c.name),
            script: t(&c.script),
            mesh: t(&c.mesh),
            icon: t(&c.icon),
            enchanting: t(&c.enchanting),
            biped_objects: c.biped_objects.iter().map(biped).collect(),
            data: gf::ClothingData {
                clothing_type: c.data.clothing_type as u32,
                weight: c.data.weight,
                value: c.data.value,
                enchantment: c.data.enchantment,
            },
        }),
        TES3Object::Book(b) => O::Book(gf::Book {
            flags: b.flags.bits(),
            id: t(&b.id),
            name: t(&b.name),
            script: t(&b.script),
            mesh: t(&b.mesh),
            icon: t(&b.icon),
            enchanting: t(&b.enchanting),
            text: t(&b.text),
            data: gf::BookData {
                weight: b.data.weight,
                value: b.data.value,
                book_type: b.data.book_type as u32,
                skill: b.data.skill as i32,
                enchantment: b.data.enchantment,
            },
        }),
        TES3Object::Alchemy(a) => O::Alchemy(gf::Alchemy {
            flags: a.flags.bits(),
            id: t(&a.id),
            name: t(&a.name),
            script: t(&a.script),
            mesh: t(&a.mesh),
            icon: t(&a.icon),
            effects: a.effects.iter().map(effect).collect(),
            data: gf::AlchemyData {
                weight: a.data.weight,
                value: a.data.value,
                flags: a.data.flags.bits(),
            },
        }),
        TES3Object::Apparatus(a) => O::Apparatus(gf::Apparatus {
            flags: a.flags.bits(),
            id: t(&a.id),
            name: t(&a.name),
            script: t(&a.script),
            mesh: t(&a.mesh),
            icon: t(&a.icon),
            data: gf::ApparatusData {
                apparatus_type: a.data.apparatus_type as u32,
                quality: a.data.quality,
                weight: a.data.weight,
                value: a.data.value,
            },
        }),
        TES3Object::Ingredient(i) => O::Ingredient(gf::Ingredient {
            flags: i.flags.bits(),
            id: t(&i.id),
            name: t(&i.name),
            script: t(&i.script),
            mesh: t(&i.mesh),
            icon: t(&i.icon),
            data: gf::IngredientData {
                weight: i.data.weight,
                value: i.data.value,
                effects: i.data.effects.map(|e| e as i32),
                skills: i.data.skills.map(|e| e as i32),
                attributes: i.data.attributes.map(|e| e as i32),
            },
        }),
        TES3Object::Lockpick(l) => O::Lockpick(gf::Lockpick {
            flags: l.flags.bits(),
            id: t(&l.id),
            name: t(&l.name),
            script: t(&l.script),
            mesh: t(&l.mesh),
            icon: t(&l.icon),
            data: gf::LockpickData {
                weight: l.data.weight,
                value: l.data.value,
                quality: l.data.quality,
                uses: l.data.uses,
            },
        }),
        TES3Object::Probe(p) => O::Probe(gf::Probe {
            flags: p.flags.bits(),
            id: t(&p.id),
            name: t(&p.name),
            script: t(&p.script),
            mesh: t(&p.mesh),
            icon: t(&p.icon),
            data: gf::ProbeData {
                weight: p.data.weight,
                value: p.data.value,
                quality: p.data.quality,
                uses: p.data.uses,
            },
        }),
        TES3Object::RepairItem(r) => O::RepairItem(gf::RepairItem {
            flags: r.flags.bits(),
            id: t(&r.id),
            name: t(&r.name),
            script: t(&r.script),
            mesh: t(&r.mesh),
            icon: t(&r.icon),
            data: gf::RepairItemData {
                weight: r.data.weight,
                value: r.data.value,
                uses: r.data.uses,
                quality: r.data.quality,
            },
        }),
        TES3Object::Creature(c) => O::Creature(gf::Creature {
            flags: c.flags.bits(),
            id: t(&c.id),
            name: t(&c.name),
            script: t(&c.script),
            mesh: t(&c.mesh),
            inventory: c.inventory.iter().map(|(n, id)| (*n, t(&id.0))).collect(),
            spells: c.spells.iter().map(|s| t(s)).collect(),
            ai_data: has(b"AIDT").then(|| ai_data(&c.ai_data)),
            ai_packages: c.ai_packages.iter().map(ai_package).collect(),
            travel_destinations: c.travel_destinations.iter().map(travel).collect(),
            sound: t(&c.sound),
            scale: c.scale,
            creature_flags: flag_word(rec).unwrap_or(c.creature_flags.bits() as u32),
            blood_type: c.blood_type,
            data: gf::CreatureData {
                creature_type: c.data.creature_type as u32,
                level: c.data.level,
                strength: c.data.strength,
                intelligence: c.data.intelligence,
                willpower: c.data.willpower,
                agility: c.data.agility,
                speed: c.data.speed,
                endurance: c.data.endurance,
                personality: c.data.personality,
                luck: c.data.luck,
                health: c.data.health,
                magicka: c.data.magicka,
                fatigue: c.data.fatigue,
                soul: c.data.soul,
                combat: c.data.combat,
                magic: c.data.magic,
                stealth: c.data.stealth,
                attack1: c.data.attack1,
                attack2: c.data.attack2,
                attack3: c.data.attack3,
                gold: c.data.gold,
            },
        }),
        TES3Object::Npc(n) => O::Npc(gf::Npc {
            flags: n.flags.bits(),
            id: t(&n.id),
            name: t(&n.name),
            script: t(&n.script),
            mesh: t(&n.mesh),
            inventory: n.inventory.iter().map(|(c, id)| (*c, t(&id.0))).collect(),
            spells: n.spells.iter().map(|s| t(s)).collect(),
            ai_data: has(b"AIDT").then(|| ai_data(&n.ai_data)),
            ai_packages: n.ai_packages.iter().map(ai_package).collect(),
            travel_destinations: n.travel_destinations.iter().map(travel).collect(),
            race: t(&n.race),
            class: t(&n.class),
            faction: t(&n.faction),
            head: t(&n.head),
            hair: t(&n.hair),
            npc_flags: flag_word(rec).unwrap_or(n.npc_flags.bits() as u32),
            blood_type: n.blood_type,
            data: gf::NpcData {
                level: n.data.level,
                stats: n.data.stats.as_ref().map(|s| gf::NpcStats {
                    attributes: s.attributes,
                    skills: s.skills,
                    health: s.health,
                    magicka: s.magicka,
                    fatigue: s.fatigue,
                }),
                disposition: n.data.disposition,
                reputation: n.data.reputation,
                rank: n.data.rank,
                gold: n.data.gold,
            },
        }),
        TES3Object::LeveledCreature(l) => O::LeveledCreature(gf::LeveledCreature {
            flags: l.flags.bits(),
            id: t(&l.id),
            leveled_creature_flags: l.leveled_creature_flags.bits(),
            chance_none: l.chance_none,
            creatures: l.creatures.iter().map(|(id, lv)| (t(id), *lv)).collect(),
        }),
        // A kind `covered` lets through always converts above; anything else is Robin's.
        _ => return gf::Object::from_raw(rec).map_err(|e| e.to_string()),
    }))
}

fn biped(b: &g7::BipedObject) -> gf::BipedObject {
    gf::BipedObject {
        biped_object_type: b.biped_object_type as u8,
        male_bodypart: t(&b.male_bodypart),
        female_bodypart: t(&b.female_bodypart),
    }
}

fn effect(e: &g7::Effect) -> gf::Effect {
    gf::Effect {
        magic_effect: e.magic_effect as i16,
        skill: e.skill as i8,
        attribute: e.attribute as i8,
        range: e.range as u32,
        area: e.area,
        duration: e.duration,
        min_magnitude: e.min_magnitude,
        max_magnitude: e.max_magnitude,
    }
}

fn ai_data(a: &g7::AiData) -> gf::AiData {
    gf::AiData {
        hello: a.hello,
        fight: a.fight,
        flee: a.flee,
        alarm: a.alarm,
        services: a.services.bits(),
    }
}

fn travel(d: &g7::TravelDestination) -> gf::TravelDestination {
    gf::TravelDestination {
        translation: d.translation,
        rotation: d.rotation,
        cell: t(&d.cell),
    }
}

fn ai_package(p: &g7::AiPackage) -> gf::AiPackage {
    match p {
        g7::AiPackage::Travel(x) => gf::AiPackage::Travel(gf::AiTravelPackage {
            location: x.location,
            reset: x.reset,
        }),
        g7::AiPackage::Wander(x) => gf::AiPackage::Wander(gf::AiWanderPackage {
            distance: x.distance,
            duration: x.duration,
            game_hour: x.game_hour,
            idle2: x.idle2,
            idle3: x.idle3,
            idle4: x.idle4,
            idle5: x.idle5,
            idle6: x.idle6,
            idle7: x.idle7,
            idle8: x.idle8,
            idle9: x.idle9,
            reset: x.reset,
        }),
        g7::AiPackage::Escort(x) => gf::AiPackage::Escort(gf::AiEscortPackage {
            location: x.location,
            duration: x.duration,
            target: t(&x.target.0),
            reset: x.reset,
            cell: t(&x.cell),
        }),
        g7::AiPackage::Follow(x) => gf::AiPackage::Follow(gf::AiFollowPackage {
            location: x.location,
            duration: x.duration,
            target: t(&x.target.0),
            reset: x.reset,
            cell: t(&x.cell),
        }),
        g7::AiPackage::Activate(x) => gf::AiPackage::Activate(gf::AiActivatePackage {
            target: t(&x.target.0),
            reset: x.reset,
        }),
    }
}

/// An actor's `FLAG` word less its blood type bits (10 to 12), as the file has it: G7
/// keeps only the flags it knows; Robin keeps every other bit so the word writes back.
fn flag_word(rec: &RawRecord) -> Option<u32> {
    let s = rec.sub(b"FLAG")?;
    Some(u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?) & !(0b111 << 10))
}

/// The FRMR dword: the master index in the top byte, the reference index below it.
fn unpack(w: u32) -> (u32, u32) {
    (w >> 24, w & 0x00ff_ffff)
}

/// A cell: G7's fields, and from the raw record the references' order, which are
/// temporary, and their moves. None when G7's map cannot hold the cell as the file has
/// it (a reference's key twice, or an `MVRF` naming another reference than the next).
fn cell(c: g7::Cell, rec: &RawRecord) -> Option<gf::Cell> {
    let mut by_key = c.references;
    let mut refs = Vec::with_capacity(by_key.len());
    let mut temporary = false;
    let mut pending: Option<(u32, u32)> = None;
    for s in &rec.subs {
        match &s.tag {
            b"NAM0" => temporary = true,
            b"MVRF" => {
                pending = Some(unpack(u32::from_le_bytes(
                    s.data.get(..4)?.try_into().ok()?,
                )))
            }
            b"FRMR" => {
                let key = unpack(u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?));
                // A key seen twice: G7 kept only the last version.
                let r = by_key.remove(&key)?;
                if let Some(word) = pending.take() {
                    if word != key {
                        return None;
                    }
                }
                refs.push(reference(r, key, temporary));
            }
            _ => {}
        }
    }
    Some(gf::Cell {
        flags: c.flags.bits(),
        name: t(&c.name),
        data: gf::CellData {
            flags: c.data.flags.bits(),
            grid: c.data.grid,
        },
        region: ot(&c.region),
        map_color: c.map_color,
        water_height: c.water_height,
        atmosphere_data: c.atmosphere_data.map(|a| gf::AtmosphereData {
            ambient_color: a.ambient_color,
            sunlight_color: a.sunlight_color,
            fog_color: a.fog_color,
            fog_density: a.fog_density,
        }),
        references: refs,
    })
}

fn reference(r: g7::Reference, key: (u32, u32), temporary: bool) -> gf::Reference {
    gf::Reference {
        mast_index: key.0,
        refr_index: key.1,
        id: t(&r.id),
        temporary,
        translation: r.translation,
        rotation: r.rotation,
        scale: r.scale,
        moved_cell: r.moved_cell,
        moved_index: None,
        owner: ot(&r.owner),
        owner_global: ot(&r.owner_global),
        owner_faction: ot(&r.owner_faction),
        owner_faction_rank: r.owner_faction_rank,
        charge_left: r.charge_left,
        health_left: r.health_left,
        object_count: r.object_count,
        destination: r.destination.as_ref().map(travel),
        lock_level: r.lock_level,
        key: ot(&r.key),
        trap: ot(&r.trap),
        soul: ot(&r.soul),
        blocked: r.blocked,
        deleted: r.deleted,
    }
}
