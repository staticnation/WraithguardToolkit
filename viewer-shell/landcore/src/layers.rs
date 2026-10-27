//! One cell's landscape, decoded, and what a plugin changed in it (`land/diff.py`).
//!
//! **A layer the record does not declare is not a layer.** `DATA` says which grids a `LAND`
//! record uses and the engine ignores the rest, so an undeclared grid - often present, full
//! of zeros - is never read as an edit: diffing it would read as *this mod flattened the
//! terrain and painted it black*.

use crate::grid::{RelativeGrid, LAND_NUM_VERTS, LAND_SIZE, NUM_TEXTURES, TEXTURE_SIZE, WNAM_SIZE};
use crate::heights::decode_heights;
use crate::meta::layer;
use crate::read::Record;

/// Every layer (`ALL_LAYERS`).
pub const ALL_LAYERS: u32 = layer::VERTEX_COLORS | layer::TEXTURES | layer::VERTEX_HEIGHTS | layer::VERTEX_NORMALS | layer::WORLD_MAP;

/// The layers a `DATA` word declares (`parse_landscape_flags` of tes3conv's spelling): the
/// three named bits, and the world map with any of them (tes3's `uses_world_map_data`).
pub fn declared_layers(flags: Option<u32>) -> u32 {
    let Some(f) = flags else { return layer::NONE };
    let mut out = layer::NONE;
    if f & 0x1 != 0 {
        out |= layer::VERTEX_HEIGHTS | layer::VERTEX_NORMALS;
    }
    if f & 0x2 != 0 {
        out |= layer::VERTEX_COLORS;
    }
    if f & 0x4 != 0 {
        out |= layer::TEXTURES;
    }
    if out & (layer::VERTEX_HEIGHTS | layer::VERTEX_COLORS | layer::TEXTURES) != 0 {
        out |= layer::WORLD_MAP;
    }
    out
}

/// One cell's layers (`LandscapeLayers`); `None` a layer not supplied, which is not the
/// same claim as zeros.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LandscapeLayers {
    pub coords: (i64, i64),
    pub declared: u32,
    pub heights: Option<Vec<i64>>,
    pub normals: Option<Vec<i64>>,
    pub world_map: Option<Vec<i64>>,
    pub colors: Option<Vec<i64>>,
    pub textures: Option<Vec<i64>>,
}

/// Why a record was not decoded: `Skip` the Python's `ValueError` (logged, the record left
/// out), `Fatal` its `LandscapeDecodeError` (a grid shorter than its format, which stops the
/// merge).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Skip(String),
    Fatal(String),
}

fn payload<'a>(raw: &'a [u8], expected: usize, what: &str) -> Result<&'a [u8], DecodeError> {
    if raw.len() < expected {
        return Err(DecodeError::Fatal(format!(
            "{what} is {} bytes, but the format requires {expected}. Refusing to render a partial grid: it would silently misplace every value after the truncation.",
            raw.len()
        )));
    }
    Ok(raw)
}

/// `VTEX` as row-major `LTEX` values, its 4x4 block order undone (`decode_texture_indices`).
pub fn decode_texture_indices(raw: &[u8]) -> Vec<i64> {
    let mut grid = vec![0i64; NUM_TEXTURES];
    for k in 0..NUM_TEXTURES {
        let v = u16::from_le_bytes([raw[2 * k], raw[2 * k + 1]]) as i64;
        let (x2, y2, x1, y1) = (k % 4, (k / 4) % 4, (k / 16) % 4, k / 64);
        grid[(y1 * 4 + y2) * TEXTURE_SIZE + x1 * 4 + x2] = v;
    }
    grid
}

/// Row-major texture values back to `VTEX`'s 4x4 block order (`pack_texture_indices`).
pub fn swizzle_texture_indices(rows: &[i64]) -> [[u16; 16]; 16] {
    let mut out = [[0u16; 16]; 16];
    for p in 0..NUM_TEXTURES {
        let (x2, y2, x1, y1) = (p % 4, (p / 4) % 4, (p / 16) % 4, p / 64);
        out[p / 16][p % 16] = rows[(y1 * 4 + y2) * TEXTURE_SIZE + x1 * 4 + x2] as u16;
    }
    out
}

impl LandscapeLayers {
    /// A `LAND` record decoded (`from_record`); `None` for an `LTEX`.
    pub fn from_record(r: &Record) -> Result<Option<LandscapeLayers>, DecodeError> {
        let Record::Land { grid, flags, heights, normals, world_map, colors, textures, .. } = r else { return Ok(None) };
        let mut out = LandscapeLayers { coords: *grid, declared: declared_layers(*flags), ..Default::default() };
        if let Some((offset, data)) = heights {
            let raw = payload(data, LAND_NUM_VERTS, "VHGT height data")?;
            out.heights = Some(decode_heights(raw, *offset as f64).map_err(DecodeError::Skip)?);
        }
        if let Some(p) = normals {
            let raw = payload(p, 3 * LAND_NUM_VERTS, "VNML normals")?;
            out.normals = Some(raw[..3 * LAND_NUM_VERTS].iter().map(|&b| b as i8 as i64).collect());
        }
        if let Some(p) = world_map {
            let raw = payload(p, WNAM_SIZE * WNAM_SIZE, "WNAM world map data")?;
            out.world_map = Some(raw[..WNAM_SIZE * WNAM_SIZE].iter().map(|&b| b as i8 as i64).collect());
        }
        if let Some(p) = colors {
            let raw = payload(p, 3 * LAND_NUM_VERTS, "VCLR colors")?;
            out.colors = Some(raw[..3 * LAND_NUM_VERTS].iter().map(|&b| b as i64).collect());
        }
        if let Some(p) = textures {
            let raw = payload(p, 2 * NUM_TEXTURES, "VTEX texture indices")?;
            out.textures = Some(decode_texture_indices(raw));
        }
        Ok(Some(out))
    }

    pub fn get(&self, which: Layer) -> &Option<Vec<i64>> {
        match which {
            Layer::Heights => &self.heights,
            Layer::Normals => &self.normals,
            Layer::WorldMap => &self.world_map,
            Layer::Colors => &self.colors,
            Layer::Textures => &self.textures,
        }
    }

    pub fn get_mut(&mut self, which: Layer) -> &mut Option<Vec<i64>> {
        match which {
            Layer::Heights => &mut self.heights,
            Layer::Normals => &mut self.normals,
            Layer::WorldMap => &mut self.world_map,
            Layer::Colors => &mut self.colors,
            Layer::Textures => &mut self.textures,
        }
    }
}

/// The five grids of a `LAND` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    Heights,
    Normals,
    WorldMap,
    Colors,
    Textures,
}

impl Layer {
    /// Each layer's side, components per vertex and bit (`_LAYERS`), in the Python's order.
    pub const DIFF_ORDER: [(Layer, usize, usize, u32); 5] = [
        (Layer::Heights, LAND_SIZE, 1, layer::VERTEX_HEIGHTS),
        (Layer::Normals, LAND_SIZE, 3, layer::VERTEX_NORMALS),
        (Layer::WorldMap, WNAM_SIZE, 1, layer::WORLD_MAP),
        (Layer::Colors, LAND_SIZE, 3, layer::VERTEX_COLORS),
        (Layer::Textures, TEXTURE_SIZE, 1, layer::TEXTURES),
    ];

    pub fn name(self) -> &'static str {
        match self {
            Layer::Heights => "heights",
            Layer::Normals => "normals",
            Layer::WorldMap => "world_map",
            Layer::Colors => "colors",
            Layer::Textures => "textures",
        }
    }
}

/// What one plugin changed in one cell (`LandscapeDiff`): a grid per layer it changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LandscapeDiff {
    pub coords: (i64, i64),
    pub plugin: String,
    pub new_land: bool,
    pub heights: Option<RelativeGrid>,
    pub normals: Option<RelativeGrid>,
    pub world_map: Option<RelativeGrid>,
    pub colors: Option<RelativeGrid>,
    pub textures: Option<RelativeGrid>,
    /// Layers declared and not supplied.
    pub missing: Vec<&'static str>,
}

impl LandscapeDiff {
    pub fn grid(&self, which: Layer) -> &Option<RelativeGrid> {
        match which {
            Layer::Heights => &self.heights,
            Layer::Normals => &self.normals,
            Layer::WorldMap => &self.world_map,
            Layer::Colors => &self.colors,
            Layer::Textures => &self.textures,
        }
    }

    fn grid_mut(&mut self, which: Layer) -> &mut Option<RelativeGrid> {
        match which {
            Layer::Heights => &mut self.heights,
            Layer::Normals => &mut self.normals,
            Layer::WorldMap => &mut self.world_map,
            Layer::Colors => &mut self.colors,
            Layer::Textures => &mut self.textures,
        }
    }

    pub fn is_modified(&self) -> bool {
        Layer::DIFF_ORDER.iter().any(|(l, ..)| self.grid(*l).as_ref().is_some_and(RelativeGrid::is_modified))
    }
}

/// What a plugin changed in a cell against the reference (`diff_against_reference`); a cell
/// the masters lack is new land, every vertex of it a change.
pub fn diff_against_reference(plugin: &str, mine: &LandscapeLayers, reference: Option<&LandscapeLayers>, allowed: u32) -> Result<LandscapeDiff, String> {
    let mut out = LandscapeDiff { coords: mine.coords, plugin: plugin.to_string(), new_land: reference.is_none(), ..Default::default() };
    let effective = allowed & mine.declared;
    for (which, side, comps, bit) in Layer::DIFF_ORDER {
        if effective & bit == 0 {
            continue;
        }
        let Some(values) = mine.get(which) else {
            out.missing.push(which.name());
            continue;
        };
        let theirs = reference.and_then(|r| r.get(which).clone()).unwrap_or_else(|| vec![0; values.len()]);
        let grid = RelativeGrid::from_difference(theirs, values, side, comps)?;
        if grid.is_modified() {
            *out.grid_mut(which) = Some(grid);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_swizzle_both_ways() {
        let rows: Vec<i64> = (0..256).collect();
        let packed = swizzle_texture_indices(&rows);
        let raw: Vec<u8> = packed.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(decode_texture_indices(&raw), rows);
        assert_eq!(packed[0][4], 16);
    }

    #[test]
    fn flags() {
        assert_eq!(declared_layers(Some(0x8)), layer::NONE);
        assert_eq!(declared_layers(Some(0x2)), layer::VERTEX_COLORS | layer::WORLD_MAP);
        assert_eq!(declared_layers(None), layer::NONE);
    }
}
