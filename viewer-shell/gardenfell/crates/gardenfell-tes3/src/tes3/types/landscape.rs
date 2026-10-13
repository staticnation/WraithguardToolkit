//! `LAND`: an exterior cell's terrain, heights, normals, colors and textures. `tes3`
//! landscape.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Landscape {
    pub flags: u32,
    pub grid: (i32, i32),
    /// `consts::landscape::*`: which of the blocks below the record says it carries. The
    /// blocks are held by presence, not by these bits: Morrowind.esm has records whose
    /// bits claim a `VTEX` or `WNAM` that is not there, and `tes3` writes zeros for those.
    /// Here a block the file has comes back and a block it lacks stays absent, whatever
    /// the bits say, so the engine's own rule for the mismatch (`world.rs`, round 18gf)
    /// can be applied by the caller.
    pub landscape_flags: u32,
    pub vertex_normals: Option<VertexNormals>,
    pub vertex_heights: Option<VertexHeights>,
    pub world_map_data: Option<WorldMapData>,
    pub vertex_colors: Option<VertexColors>,
    pub texture_indices: Option<TextureIndices>,
}

/// `VNML`, 12675 bytes: a signed byte normal per vertex, 65 by 65, rows of `[x, y, z]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexNormals {
    pub data: Box<[[[i8; 3]; 65]; 65]>,
}

impl Default for VertexNormals {
    fn default() -> Self {
        VertexNormals { data: Box::new([[[0; 3]; 65]; 65]) }
    }
}

impl VertexNormals {
    pub const SIZE: usize = 12675;

    pub fn read(r: &mut Reader<'_>) -> Result<VertexNormals> {
        let bytes = r.bytes(Self::SIZE)?;
        let mut data = Box::new([[[0i8; 3]; 65]; 65]);
        // Row by row and vertex by vertex, in loops the compiler can unroll: the engine
        // reads 1,390 of these per master (round 18gh), and a flattened iterator over
        // 12,675 bytes was a tenth of its parse.
        for (row, src) in data.iter_mut().zip(bytes.chunks_exact(3 * 65)) {
            for (v, b) in row.iter_mut().zip(src.chunks_exact(3)) {
                if let Ok(b) = <&[u8; 3]>::try_from(b) {
                    *v = [b[0] as i8, b[1] as i8, b[2] as i8];
                }
            }
        }
        Ok(VertexNormals { data })
    }

    pub fn write(&self, w: &mut Writer) {
        for v in self.data.iter().flatten().flatten() {
            w.i8(*v);
        }
    }
}

/// `VHGT`, 4232 bytes: a base height, then 65 by 65 signed byte deltas (each row starts
/// from the first vertex of the row before, each vertex from its left neighbor), then 3
/// padding bytes. Heights are in units of 8.
#[derive(Clone, Debug, PartialEq)]
pub struct VertexHeights {
    pub offset: f32,
    pub data: Box<[[i8; 65]; 65]>,
}

impl Default for VertexHeights {
    fn default() -> Self {
        VertexHeights { offset: 0.0, data: Box::new([[0; 65]; 65]) }
    }
}

impl VertexHeights {
    pub const SIZE: usize = 4232;

    pub fn read(r: &mut Reader<'_>) -> Result<VertexHeights> {
        let offset = r.f32()?;
        let bytes = r.bytes(65 * 65)?;
        let mut data = Box::new([[0i8; 65]; 65]);
        for (row, src) in data.iter_mut().zip(bytes.chunks_exact(65)) {
            if let Ok(src) = <&[u8; 65]>::try_from(src) {
                for (v, b) in row.iter_mut().zip(src) {
                    *v = *b as i8;
                }
            }
        }
        r.skip(3)?;
        Ok(VertexHeights { offset, data })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.offset);
        for v in self.data.iter().flatten() {
            w.i8(*v);
        }
        w.bytes(&[0; 3]);
    }
}

/// `WNAM`, 81 bytes: the 9 by 9 signed byte heights the world map draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldMapData {
    pub data: Box<[[i8; 9]; 9]>,
}

impl WorldMapData {
    pub const SIZE: usize = 81;

    pub fn read(r: &mut Reader<'_>) -> Result<WorldMapData> {
        let bytes = r.bytes(Self::SIZE)?;
        let mut data = Box::new([[0i8; 9]; 9]);
        for (v, b) in data.iter_mut().flatten().zip(bytes) {
            *v = *b as i8;
        }
        Ok(WorldMapData { data })
    }

    pub fn write(&self, w: &mut Writer) {
        for v in self.data.iter().flatten() {
            w.i8(*v);
        }
    }
}

/// `VCLR`, 12675 bytes: an RGB byte color per vertex, 65 by 65.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexColors {
    pub data: Box<[[[u8; 3]; 65]; 65]>,
}

impl Default for VertexColors {
    fn default() -> Self {
        VertexColors { data: Box::new([[[0; 3]; 65]; 65]) }
    }
}

impl VertexColors {
    pub const SIZE: usize = 12675;

    pub fn read(r: &mut Reader<'_>) -> Result<VertexColors> {
        let bytes = r.bytes(Self::SIZE)?;
        let mut data = Box::new([[[0u8; 3]; 65]; 65]);
        for (row, src) in data.iter_mut().zip(bytes.chunks_exact(3 * 65)) {
            for (v, b) in row.iter_mut().zip(src.chunks_exact(3)) {
                if let Ok(b) = <&[u8; 3]>::try_from(b) {
                    *v = *b;
                }
            }
        }
        Ok(VertexColors { data })
    }

    pub fn write(&self, w: &mut Writer) {
        for v in self.data.iter().flatten().flatten() {
            w.u8(*v);
        }
    }
}

/// `VTEX`, 512 bytes: a `u16` landscape texture index per 16 by 16 texel, stored as the
/// file holds them (in 4 by 4 blocks of 4 by 4 texels, not plain rows).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextureIndices {
    pub data: Box<[[u16; 16]; 16]>,
}

impl TextureIndices {
    pub const SIZE: usize = 512;

    pub fn read(r: &mut Reader<'_>) -> Result<TextureIndices> {
        let mut data = Box::new([[0u16; 16]; 16]);
        for v in data.iter_mut().flatten() {
            *v = r.u16()?;
        }
        Ok(TextureIndices { data })
    }

    pub fn write(&self, w: &mut Writer) {
        for v in self.data.iter().flatten() {
            w.u16(*v);
        }
    }
}

impl Typed for Landscape {
    const TAG: Tag = *b"LAND";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"INTV" => {
                    let mut r = sized(rec, s, 8)?;
                    this.grid = (r.i32()?, r.i32()?);
                }
                b"DATA" => this.landscape_flags = u32_of(rec, s)?,
                b"VNML" => {
                    this.vertex_normals = Some(VertexNormals::read(&mut sized(rec, s, VertexNormals::SIZE)?)?)
                }
                b"VHGT" => {
                    this.vertex_heights = Some(VertexHeights::read(&mut sized(rec, s, VertexHeights::SIZE)?)?)
                }
                b"WNAM" => {
                    this.world_map_data = Some(WorldMapData::read(&mut sized(rec, s, WorldMapData::SIZE)?)?)
                }
                b"VCLR" => {
                    this.vertex_colors = Some(VertexColors::read(&mut sized(rec, s, VertexColors::SIZE)?)?)
                }
                b"VTEX" => {
                    this.texture_indices =
                        Some(TextureIndices::read(&mut sized(rec, s, TextureIndices::SIZE)?)?)
                }
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.sub(b"INTV", |w| {
            w.i32(self.grid.0);
            w.i32(self.grid.1);
            Ok(())
        })?;
        out.u32(b"DATA", self.landscape_flags);
        // Each block the record holds, in the order the Construction Set writes them.
        if let Some(v) = &self.vertex_normals {
            out.sub(b"VNML", |w| {
                v.write(w);
                Ok(())
            })?;
        }
        if let Some(v) = &self.vertex_heights {
            out.sub(b"VHGT", |w| {
                v.write(w);
                Ok(())
            })?;
        }
        if let Some(v) = &self.world_map_data {
            out.sub(b"WNAM", |w| {
                v.write(w);
                Ok(())
            })?;
        }
        if let Some(v) = &self.vertex_colors {
            out.sub(b"VCLR", |w| {
                v.write(w);
                Ok(())
            })?;
        }
        if let Some(v) = &self.texture_indices {
            out.sub(b"VTEX", |w| {
                v.write(w);
                Ok(())
            })?;
        }
        out.dele();
        Ok(out.finish())
    }
}

impl Landscape {
    /// The flag bits a record's blocks would set, for comparing with `landscape_flags`.
    pub fn flags_of_blocks(&self) -> u32 {
        let mut f = 0;
        if self.vertex_heights.is_some() || self.vertex_normals.is_some() {
            f |= consts::landscape::USES_VERTEX_HEIGHTS_AND_NORMALS;
        }
        if self.vertex_colors.is_some() {
            f |= consts::landscape::USES_VERTEX_COLORS;
        }
        if self.texture_indices.is_some() {
            f |= consts::landscape::USES_TEXTURES;
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_landscape_round_trips_and_a_short_vhgt_is_refused() {
        let mut rec = Record::new(b"LAND");
        rec.push(b"INTV", [(-2i32).to_le_bytes(), 7i32.to_le_bytes()].concat());
        let all = consts::landscape::USES_VERTEX_HEIGHTS_AND_NORMALS
            | consts::landscape::USES_VERTEX_COLORS
            | consts::landscape::USES_TEXTURES;
        rec.push(b"DATA", all.to_le_bytes().to_vec());
        let mut vnml = vec![0u8; VertexNormals::SIZE];
        vnml[2] = 127;
        vnml[VertexNormals::SIZE - 1] = 0xFF;
        rec.push(b"VNML", vnml);
        let mut vhgt = Vec::with_capacity(VertexHeights::SIZE);
        vhgt.extend_from_slice(&(-10.5f32).to_le_bytes());
        vhgt.extend((0..65 * 65).map(|i| (i % 7) as u8));
        vhgt.extend_from_slice(&[0; 3]);
        rec.push(b"VHGT", vhgt);
        rec.push(b"WNAM", (0..81).map(|i| i as u8).collect());
        let mut vclr = vec![255u8; VertexColors::SIZE];
        vclr[3] = 1;
        rec.push(b"VCLR", vclr);
        let vtex: Vec<u8> = (0..256u16).flat_map(|i| (i + 1).to_le_bytes()).collect();
        rec.push(b"VTEX", vtex);
        let l = Landscape::from_raw(&rec).unwrap();
        assert_eq!(l.grid, (-2, 7));
        assert_eq!(l.landscape_flags, all);
        let normals = l.vertex_normals.as_ref().unwrap();
        assert_eq!(normals.data[0][0], [0, 0, 127]);
        assert_eq!(normals.data[64][64][2], -1);
        let heights = l.vertex_heights.as_ref().unwrap();
        assert_eq!(heights.offset, -10.5);
        assert_eq!(heights.data[0][1], 1);
        assert_eq!(heights.data[1][0], (65 % 7) as i8);
        assert_eq!(l.world_map_data.as_ref().unwrap().data[8][8], 80);
        assert_eq!(l.vertex_colors.as_ref().unwrap().data[0][1], [1, 255, 255]);
        assert_eq!(l.texture_indices.as_ref().unwrap().data[15][15], 256);
        assert_eq!(l.flags_of_blocks(), all);
        assert_eq!(l.to_raw().unwrap(), rec);

        // A record with no blocks writes only INTV and DATA, whatever its bits say.
        let bare = Landscape { grid: (1, 1), landscape_flags: all, ..Default::default() };
        let tags: Vec<Tag> = bare.to_raw().unwrap().subs.iter().map(|s| s.tag).collect();
        assert_eq!(tags, vec![*b"INTV", *b"DATA"]);
        assert_eq!(bare.flags_of_blocks(), 0);

        rec.subs[3].data.to_mut().truncate(VertexHeights::SIZE - 1);
        let e = Landscape::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("LAND::VHGT is 4231 bytes"), "{e}");
    }
}
