//! `NiParticleSystemController`: the emitter and the particle state of a particle system,
//! and the heads of its modifier and collider chains. `tes3` niparticlesystemcontroller.rs.
//!
//! The one place this layer departs from `tes3` (`backend-audit.md` D14): after the spawn
//! fields the file holds a `u16` particle count, then the `u16` active count, then the
//! particles, 40 bytes each. `tes3` reads the active count after the particles, so it
//! takes the same number of bytes but reads every particle two bytes off and the active
//! count out of the last particle's tail. nif.xml and the engine's own reader (`nif.rs`
//! `read_particle_ctrl`), checked against the game's `ashcloud.nif`, have the
//! count-count-particles order used here, and the struct lists its fields in that order.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticleSystemController {
    pub base: NiTimeController,
    pub speed: f32,
    pub speed_variation: f32,
    pub declination_angle: f32,
    pub declination_variation: f32,
    pub planar_angle: f32,
    pub planar_angle_variation: f32,
    pub initial_normal: [f32; 3],
    /// RGBA.
    pub initial_color: [f32; 4],
    pub initial_size: f32,
    pub emit_start_time: f32,
    pub emit_stop_time: f32,
    /// One byte in the file.
    pub reset_particle_system: bool,
    pub birth_rate: f32,
    pub lifespan: f32,
    pub lifespan_variation: f32,
    /// One byte in the file.
    pub use_birth_rate: bool,
    /// One byte in the file.
    pub spawn_on_death: bool,
    pub emitter_width: f32,
    pub emitter_height: f32,
    pub emitter_depth: f32,
    pub emitter: Link<NiAVObject>,
    pub spawn_generations: u16,
    pub spawn_percentage: f32,
    pub spawn_multiplier: u16,
    pub spawned_speed_chaos: f32,
    pub spawned_direction_chaos: f32,
    /// Read before the particles, in the file's order (D14), where `tes3` has it after.
    pub num_active_particles: u16,
    /// The file's `u16` count is `particles.len()`.
    pub particles: Vec<NiPerParticleData>,
    pub emitter_modifier: Link<NiEmitterModifier>,
    pub particle_modifier: Link<NiParticleModifier>,
    pub particle_collider: Link<NiParticleCollider>,
    /// One byte in the file.
    pub compute_dynamic_bounding_volume: bool,
}

impl Block for NiParticleSystemController {
    const NAME: &'static str = "NiParticleSystemController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiTimeController::read(r)?;
        let speed = r.f32()?;
        let speed_variation = r.f32()?;
        let declination_angle = r.f32()?;
        let declination_variation = r.f32()?;
        let planar_angle = r.f32()?;
        let planar_angle_variation = r.f32()?;
        let initial_normal = r.vec3()?;
        let initial_color = r.vec4()?;
        let initial_size = r.f32()?;
        let emit_start_time = r.f32()?;
        let emit_stop_time = r.f32()?;
        let reset_particle_system = r.bool_u8()?;
        let birth_rate = r.f32()?;
        let lifespan = r.f32()?;
        let lifespan_variation = r.f32()?;
        let use_birth_rate = r.bool_u8()?;
        let spawn_on_death = r.bool_u8()?;
        let emitter_width = r.f32()?;
        let emitter_height = r.f32()?;
        let emitter_depth = r.f32()?;
        let emitter = r.link()?;
        let spawn_generations = r.u16()?;
        let spawn_percentage = r.f32()?;
        let spawn_multiplier = r.u16()?;
        let spawned_speed_chaos = r.f32()?;
        let spawned_direction_chaos = r.f32()?;
        // D14: the particle count, then the active count, then the particles.
        let num_particles = r.u16()? as usize;
        if num_particles * NiPerParticleData::SIZE > r.remaining() {
            return r.error(format!(
                "a count of {num_particles} particles of {} bytes with {} bytes left",
                NiPerParticleData::SIZE,
                r.remaining()
            ));
        }
        let num_active_particles = r.u16()?;
        let particles = r.seq(num_particles, NiPerParticleData::read)?;
        let emitter_modifier = r.link()?;
        let particle_modifier = r.link()?;
        let particle_collider = r.link()?;
        let compute_dynamic_bounding_volume = r.bool_u8()?;
        Ok(NiParticleSystemController {
            base,
            speed,
            speed_variation,
            declination_angle,
            declination_variation,
            planar_angle,
            planar_angle_variation,
            initial_normal,
            initial_color,
            initial_size,
            emit_start_time,
            emit_stop_time,
            reset_particle_system,
            birth_rate,
            lifespan,
            lifespan_variation,
            use_birth_rate,
            spawn_on_death,
            emitter_width,
            emitter_height,
            emitter_depth,
            emitter,
            spawn_generations,
            spawn_percentage,
            spawn_multiplier,
            spawned_speed_chaos,
            spawned_direction_chaos,
            num_active_particles,
            particles,
            emitter_modifier,
            particle_modifier,
            particle_collider,
            compute_dynamic_bounding_volume,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.speed);
        w.f32(self.speed_variation);
        w.f32(self.declination_angle);
        w.f32(self.declination_variation);
        w.f32(self.planar_angle);
        w.f32(self.planar_angle_variation);
        w.vec3(&self.initial_normal);
        w.vec4(&self.initial_color);
        w.f32(self.initial_size);
        w.f32(self.emit_start_time);
        w.f32(self.emit_stop_time);
        w.bool_u8(self.reset_particle_system);
        w.f32(self.birth_rate);
        w.f32(self.lifespan);
        w.f32(self.lifespan_variation);
        w.bool_u8(self.use_birth_rate);
        w.bool_u8(self.spawn_on_death);
        w.f32(self.emitter_width);
        w.f32(self.emitter_height);
        w.f32(self.emitter_depth);
        w.link(self.emitter);
        w.u16(self.spawn_generations);
        w.f32(self.spawn_percentage);
        w.u16(self.spawn_multiplier);
        w.f32(self.spawned_speed_chaos);
        w.f32(self.spawned_direction_chaos);
        // D14: the particle count, then the active count, then the particles.
        w.u16(self.particles.len() as u16);
        w.u16(self.num_active_particles);
        for p in &self.particles {
            p.write(w);
        }
        w.link(self.emitter_modifier);
        w.link(self.particle_modifier);
        w.link(self.particle_collider);
        w.bool_u8(self.compute_dynamic_bounding_volume);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.emitter.index);
        f(&mut self.emitter_modifier.index);
        f(&mut self.particle_modifier.index);
        f(&mut self.particle_collider.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes from the block's start to the particle count: the time controller (26),
    /// then the emitter fields.
    const COUNT_AT: usize = 26 + 6 * 4 + 12 + 16 + 3 * 4 + 1 + 3 * 4 + 2 + 3 * 4 + 4 + 2 + 4 + 2 + 4 + 4;

    #[test]
    fn a_controller_round_trips_in_the_files_order_and_a_bad_count_is_refused() {
        let c = NiParticleSystemController {
            speed: 10.0,
            initial_color: [1.0, 0.5, 0.25, 1.0],
            emit_stop_time: 5.0,
            use_birth_rate: true,
            emitter: Link::new(2),
            spawn_generations: 1,
            num_active_particles: 1,
            particles: vec![
                NiPerParticleData { age: 0.5, lifespan: 2.0, index: 0, ..Default::default() },
                NiPerParticleData { index: 1, ..Default::default() },
            ],
            particle_modifier: Link::new(5),
            compute_dynamic_bounding_volume: true,
            ..Default::default()
        };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), COUNT_AT + 2 + 2 + 2 * 40 + 3 * 4 + 1);
        // The count comes first, the active count second.
        assert_eq!(&w.out[COUNT_AT..COUNT_AT + 4], &[2, 0, 1, 0]);
        assert_eq!(NiParticleSystemController::read(&mut Reader::new(&w.out)).unwrap(), c);
        let mut bad = w.out.clone();
        bad[COUNT_AT..COUNT_AT + 2].copy_from_slice(&500u16.to_le_bytes());
        let e = NiParticleSystemController::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("500 particles"), "{e}");
    }
}
