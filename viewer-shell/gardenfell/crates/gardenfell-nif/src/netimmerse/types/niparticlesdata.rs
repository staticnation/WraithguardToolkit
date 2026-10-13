//! `NiParticlesData`: geometry data for a particle system, with the particle count, the
//! radius, the count alive and an optional size per particle. `tes3` niparticlesdata.rs.
//!
//! After the geometry arrays the file holds a `u16` particle count, the radius, a `u16`
//! active count, then a has-sizes word and `vertices.len()` sizes when it is set. `tes3`
//! and the engine's own reader (`nif.rs` `read_particle_data`, measured against the game's
//! `ashcloud.nif`) agree on this order, so there is nothing to decide here.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticlesData {
    pub base: NiGeometryData,
    /// The file's own word, usually `vertices.len()`.
    pub num_particles: u16,
    pub particle_radius: f32,
    pub num_active: u16,
    /// Empty, or one per vertex.
    pub sizes: Vec<f32>,
}

impl Block for NiParticlesData {
    const NAME: &'static str = "NiParticlesData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiGeometryData::read(r)?;
        let num_particles = r.u16()?;
        let particle_radius = r.f32()?;
        let num_active = r.u16()?;
        let has_sizes = r.bool_u32()?;
        let sizes = r.f32s_n(if has_sizes { base.vertices.len() } else { 0 })?;
        Ok(NiParticlesData { base, num_particles, particle_radius, num_active, sizes })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.num_particles);
        w.f32(self.particle_radius);
        w.u16(self.num_active);
        w.bool_u32(!self.sizes.is_empty());
        w.f32s(&self.sizes);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_data_round_trips_with_and_without_sizes() {
        let mut d = NiParticlesData::default();
        d.base.vertices = vec![[0.0; 3]; 3];
        d.num_particles = 3;
        d.particle_radius = 2.5;
        d.num_active = 2;
        d.sizes = vec![1.0, 2.0, 3.0];
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 2 + 4 + 36 + 4 + 16 + 4 + 2 + 4 + 2 + 4 + 2 + 4 + 12);
        assert_eq!(NiParticlesData::read(&mut Reader::new(&w.out)).unwrap(), d);
        d.sizes.clear();
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiParticlesData::read(&mut Reader::new(&w.out)).unwrap(), d);
        // The has-sizes word set with no sizes behind it.
        let mut bad = w.out.clone();
        let n = bad.len();
        bad[n - 4..].copy_from_slice(&1u32.to_le_bytes());
        let e = NiParticlesData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("4 bytes wanted, 0 left"), "{e}");
    }
}
