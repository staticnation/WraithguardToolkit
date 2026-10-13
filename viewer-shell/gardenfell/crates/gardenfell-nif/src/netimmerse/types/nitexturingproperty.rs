//! `NiTexturingProperty`: the texture slots of a shape and how they are applied. `tes3`
//! nitexturingproperty.rs.
//!
//! The file holds an apply mode, a `u32` slot count, then per slot a has-map word and,
//! when set, the map. The slots are base, dark, detail, gloss, glow, bump and decal in
//! that order, and the bump slot (index 5) carries a longer `BumpMap`. `Map` and
//! `BumpMap` are shared structures with `tes3`'s names, not blocks.

use super::*;

/// The slot a `BumpMap` is read from; every other slot holds a `Map`.
pub const BUMP_INDEX: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct NiTexturingProperty {
    pub base: NiProperty,
    /// `consts::apply_mode::*`
    pub apply_mode: i32,
    /// One entry per slot the file names, `None` where the has-map word was zero.
    pub texture_maps: Vec<Option<TextureMap>>,
}

impl Default for NiTexturingProperty {
    fn default() -> Self {
        // `tes3`'s enum default is `Modulate`.
        NiTexturingProperty { base: Default::default(), apply_mode: 2, texture_maps: Vec::new() }
    }
}

impl Block for NiTexturingProperty {
    const NAME: &'static str = "NiTexturingProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiProperty::read(r)?;
        let apply_mode = r.i32()?;
        // A slot is at least its has-map word.
        let num_texture_maps = r.count(4)?;
        let mut texture_maps = Vec::with_capacity(num_texture_maps);
        for i in 0..num_texture_maps {
            let has_map = r.bool_u32()?;
            texture_maps.push(if !has_map {
                None
            } else if i == BUMP_INDEX {
                Some(TextureMap::BumpMap(BumpMap::read(r)?))
            } else {
                Some(TextureMap::Map(Map::read(r)?))
            });
        }
        Ok(NiTexturingProperty { base, apply_mode, texture_maps })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.i32(self.apply_mode);
        w.count(self.texture_maps.len());
        for slot in &self.texture_maps {
            w.bool_u32(slot.is_some());
            match slot {
                None => {}
                Some(TextureMap::Map(map)) => map.write(w)?,
                Some(TextureMap::BumpMap(map)) => map.write(w)?,
            }
        }
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        for slot in self.texture_maps.iter_mut().flatten() {
            slot.links(f);
        }
    }
}

/// One filled slot: a plain map, or the bump slot's longer one.
#[derive(Clone, Debug, PartialEq)]
pub enum TextureMap {
    Map(Map),
    BumpMap(BumpMap),
}

impl Default for TextureMap {
    fn default() -> Self {
        TextureMap::Map(Map::default())
    }
}

impl TextureMap {
    pub fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        match self {
            TextureMap::Map(m) => m.links(f),
            TextureMap::BumpMap(b) => b.links(f),
        }
    }
}

/// A texture slot: the texture and how it is sampled.
#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    pub base: NiObject,
    pub texture: Link<NiSourceTexture>,
    /// `consts::clamp_mode::*`
    pub clamp_mode: i32,
    /// `consts::filter_mode::*`
    pub filter_mode: i32,
    /// Which UV set of the geometry data this slot samples with.
    pub texture_index: u32,
    pub ps2_l: i16,
    pub ps2_k: i16,
    pub unknown_flag1: u8,
    pub unknown_flag2: u8,
}

impl Default for Map {
    fn default() -> Self {
        // `tes3`'s defaults: `WrapSWrapT`, `Trilerp`, and the PS2 K every exporter writes.
        Map {
            base: NiObject,
            texture: Link::NONE,
            clamp_mode: 3,
            filter_mode: 2,
            texture_index: 0,
            ps2_l: 0,
            ps2_k: -75,
            unknown_flag1: 0,
            unknown_flag2: 0,
        }
    }
}

impl Map {
    pub fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        f(&mut self.texture.index);
    }
}

impl Map {
    pub fn read(r: &mut Reader<'_>) -> Result<Map> {
        Ok(Map {
            base: NiObject::read(r)?,
            texture: r.link()?,
            clamp_mode: r.i32()?,
            filter_mode: r.i32()?,
            texture_index: r.u32()?,
            ps2_l: r.i16()?,
            ps2_k: r.i16()?,
            unknown_flag1: r.u8()?,
            unknown_flag2: r.u8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.texture);
        w.i32(self.clamp_mode);
        w.i32(self.filter_mode);
        w.u32(self.texture_index);
        w.i16(self.ps2_l);
        w.i16(self.ps2_k);
        w.u8(self.unknown_flag1);
        w.u8(self.unknown_flag2);
        Ok(())
    }
}

/// The bump slot's map, with the luma scale and offset and the displacement matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct BumpMap {
    pub base: Map,
    pub luma_scale: f32,
    pub luma_offset: f32,
    /// The file's 2x2, row by row.
    pub displacement: [[f32; 2]; 2],
}

impl Default for BumpMap {
    fn default() -> Self {
        BumpMap {
            base: Map::default(),
            luma_scale: 0.0,
            luma_offset: 0.0,
            displacement: [[1.0, 0.0], [0.0, 1.0]],
        }
    }
}

impl BumpMap {
    pub fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

impl BumpMap {
    pub fn read(r: &mut Reader<'_>) -> Result<BumpMap> {
        Ok(BumpMap {
            base: Map::read(r)?,
            luma_scale: r.f32()?,
            luma_offset: r.f32()?,
            displacement: [r.vec2()?, r.vec2()?],
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.luma_scale);
        w.f32(self.luma_offset);
        w.vec2(&self.displacement[0]);
        w.vec2(&self.displacement[1]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texturing_property_round_trips_with_a_bump_slot_and_a_bad_count_is_refused() {
        let mut t = NiTexturingProperty::default();
        t.texture_maps = vec![
            Some(TextureMap::Map(Map { texture: Link::new(3), ..Default::default() })),
            None,
            None,
            None,
            Some(TextureMap::Map(Map { texture: Link::new(4), texture_index: 1, ..Default::default() })),
            Some(TextureMap::BumpMap(BumpMap {
                base: Map { texture: Link::new(5), ..Default::default() },
                luma_scale: 1.0,
                luma_offset: 0.5,
                displacement: [[1.0, 0.0], [0.0, 1.0]],
            })),
            None,
        ];
        let mut w = Writer::new();
        t.write(&mut w).unwrap();
        let header = 4 + 4 + 4 + 2 + 4;
        // A `Map` is 22 bytes, a `BumpMap` 46.
        assert_eq!(w.out.len(), header + 4 + 7 * 4 + 2 * 22 + (22 + 8 + 16));
        assert_eq!(NiTexturingProperty::read(&mut Reader::new(&w.out)).unwrap(), t);
        // A slot count past the end of the block is refused before anything is read.
        let mut bad = w.out.clone();
        bad[header..header + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiTexturingProperty::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
