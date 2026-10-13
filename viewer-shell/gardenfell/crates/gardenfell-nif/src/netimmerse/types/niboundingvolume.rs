//! `NiBoundingVolume`: the optional bounding volume of an `NiAVObject`, one of three
//! shapes told apart by a leading type word (`consts::bound_type::*`). `tes3`
//! niboundingvolume.rs, which wraps the same three in a struct.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum NiBoundingVolume {
    Box(NiBoxBV),
    Sphere(NiSphereBV),
    Union(NiUnionBV),
}

impl Default for NiBoundingVolume {
    fn default() -> Self {
        NiBoundingVolume::Box(NiBoxBV::default())
    }
}

impl NiBoundingVolume {
    pub fn read(r: &mut Reader<'_>) -> Result<NiBoundingVolume> {
        let at = r.pos();
        let kind = r.i32()?;
        Ok(match kind {
            consts::bound_type::BOX => NiBoundingVolume::Box(NiBoxBV::read(r)?),
            consts::bound_type::SPHERE => NiBoundingVolume::Sphere(NiSphereBV::read(r)?),
            consts::bound_type::UNION => NiBoundingVolume::Union(NiUnionBV::read(r)?),
            _ => {
                return Err(crate::bytes::Error {
                    at,
                    what: format!("bounding volume type {kind} is not box, sphere or union"),
                })
            }
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        match self {
            NiBoundingVolume::Box(b) => {
                w.i32(consts::bound_type::BOX);
                b.write(w)
            }
            NiBoundingVolume::Sphere(s) => {
                w.i32(consts::bound_type::SPHERE);
                s.write(w)
            }
            NiBoundingVolume::Union(u) => {
                w.i32(consts::bound_type::UNION);
                u.write(w)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shape_round_trips_and_an_unknown_type_is_refused() {
        let u = NiBoundingVolume::Union(NiUnionBV {
            bounding_volumes: vec![
                NiBoundingVolume::Sphere(NiSphereBV { center: [1.0, 2.0, 3.0], radius: 4.0 }),
                NiBoundingVolume::Box(NiBoxBV {
                    center: [0.0; 3],
                    axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    extents: [1.0, 2.0, 3.0],
                }),
            ],
        });
        let mut w = Writer::new();
        u.write(&mut w).unwrap();
        assert_eq!(NiBoundingVolume::read(&mut Reader::new(&w.out)).unwrap(), u);
        let e = NiBoundingVolume::read(&mut Reader::new(&7i32.to_le_bytes())).unwrap_err();
        assert!(e.what.contains("type 7"), "{e}");
    }
}
