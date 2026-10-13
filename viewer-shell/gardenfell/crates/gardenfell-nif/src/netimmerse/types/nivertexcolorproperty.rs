//! `NiVertexColorProperty`: how vertex colors take part in lighting. `tes3`
//! nivertexcolorproperty.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiVertexColorProperty {
    pub base: NiProperty,
    /// `consts::source_vertex_mode::*`
    pub source_vertex_mode: i32,
    /// `consts::lighting_mode::*`
    pub lighting_mode: i32,
}

impl Default for NiVertexColorProperty {
    fn default() -> Self {
        // `tes3`'s enum defaults: `Ignore` and `EmissiveAmbientDiffuse`.
        NiVertexColorProperty { base: Default::default(), source_vertex_mode: 0, lighting_mode: 1 }
    }
}

impl Block for NiVertexColorProperty {
    const NAME: &'static str = "NiVertexColorProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiVertexColorProperty {
            base: NiProperty::read(r)?,
            source_vertex_mode: r.i32()?,
            lighting_mode: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.i32(self.source_vertex_mode);
        w.i32(self.lighting_mode);
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
    fn a_vertex_color_property_round_trips() {
        let p = NiVertexColorProperty { source_vertex_mode: 2, ..Default::default() };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 4 + 2 + 4 + 4);
        assert_eq!(NiVertexColorProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
