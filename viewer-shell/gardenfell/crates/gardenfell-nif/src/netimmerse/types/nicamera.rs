//! `NiCamera`: a camera, its frustum and viewport, the scene it looks at and the screen
//! polygons drawn over it. `tes3` nicamera.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiCamera {
    pub base: NiAVObject,
    /// An `NiFrustum`: left, right, top, bottom, near, far.
    pub view_frustum: [f32; 6],
    /// An `NiRect`: left, right, top, bottom, as fractions of the screen.
    pub view_port: [f32; 4],
    pub lod_adjust: f32,
    pub scene: Link<NiNode>,
    pub screen_polygons: Vec<Link<NiScreenPolygon>>,
}

impl Block for NiCamera {
    const NAME: &'static str = "NiCamera";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiCamera {
            base: NiAVObject::read(r)?,
            view_frustum: r.f32s::<6>()?,
            view_port: r.f32s::<4>()?,
            lod_adjust: r.f32()?,
            scene: r.link()?,
            screen_polygons: r.links()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32s(&self.view_frustum);
        w.f32s(&self.view_port);
        w.f32(self.lod_adjust);
        w.link(self.scene);
        w.links(&self.screen_polygons);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.scene.index);
        for l in &mut self.screen_polygons {
            f(&mut l.index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_camera_round_trips_and_a_bad_polygon_count_is_refused() {
        let c = NiCamera {
            view_frustum: [-1.0, 1.0, 1.0, -1.0, 1.0, 1000.0],
            view_port: [0.0, 1.0, 1.0, 0.0],
            lod_adjust: 1.0,
            scene: Link::new(0),
            screen_polygons: vec![Link::new(5), Link::new(6)],
            ..Default::default()
        };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiCamera::read(&mut Reader::new(&w.out)).unwrap(), c);
        // The polygon count is the last u32 but two links.
        let mut bad = w.out.clone();
        let at = bad.len() - 12;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiCamera::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
