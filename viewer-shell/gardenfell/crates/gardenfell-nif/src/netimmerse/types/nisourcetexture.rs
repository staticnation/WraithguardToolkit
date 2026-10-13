//! `NiSourceTexture`: a texture from a file on disk or from pixel data in the stream, with
//! the format hints the renderer gets. `tes3` nisourcetexture.rs.
//!
//! `TextureSource` is the shared structure the source is read as, also carried by
//! `NiBltSource`: a has-external byte and the file name when set, otherwise a has-internal
//! byte and the `NiPixelData` link when set. A file with neither reads as an internal
//! source with no link, and as `tes3` does, that is written back without the link, so
//! the one shape that does not round trip is has-internal set with a link of -1.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiSourceTexture {
    pub base: NiTexture,
    pub source: TextureSource,
    /// `consts::pixel_layout::*`
    pub pixel_layout: i32,
    /// `consts::use_mipmaps::*`
    pub use_mipmaps: i32,
    /// `consts::alpha_format::*`
    pub alpha_format: i32,
    /// One byte in the file.
    pub is_static: bool,
}

impl Default for NiSourceTexture {
    fn default() -> Self {
        // `tes3`'s defaults: the three `Default` hints, and static.
        NiSourceTexture {
            base: Default::default(),
            source: TextureSource::default(),
            pixel_layout: 6,
            use_mipmaps: 2,
            alpha_format: 3,
            is_static: true,
        }
    }
}

impl Block for NiSourceTexture {
    const NAME: &'static str = "NiSourceTexture";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSourceTexture {
            base: NiTexture::read(r)?,
            source: TextureSource::read(r)?,
            pixel_layout: r.i32()?,
            use_mipmaps: r.i32()?,
            alpha_format: r.i32()?,
            is_static: r.bool_u8()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        self.source.write(w)?;
        w.i32(self.pixel_layout);
        w.i32(self.use_mipmaps);
        w.i32(self.alpha_format);
        w.bool_u8(self.is_static);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        self.source.links(f);
    }
}

/// Where a texture's pixels come from: a file name, or pixel data in the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureSource {
    External(String),
    Internal(Link<NiPixelData>),
}

impl Default for TextureSource {
    fn default() -> Self {
        TextureSource::Internal(Link::NONE)
    }
}

impl TextureSource {
    pub fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        if let TextureSource::Internal(l) = self {
            f(&mut l.index);
        }
    }
}

impl TextureSource {
    pub fn read(r: &mut Reader<'_>) -> Result<TextureSource> {
        if r.bool_u8()? {
            return Ok(TextureSource::External(r.string()?));
        }
        if r.bool_u8()? {
            return Ok(TextureSource::Internal(r.link()?));
        }
        Ok(TextureSource::Internal(Link::NONE))
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        match self {
            TextureSource::External(file_name) => {
                w.u8(1);
                w.string(file_name)?;
            }
            TextureSource::Internal(pixel_data) => {
                w.u8(0);
                if pixel_data.is_none() {
                    w.u8(0);
                } else {
                    w.u8(1);
                    w.link(*pixel_data);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_texture_source_round_trips() {
        let external =
            NiSourceTexture { source: TextureSource::External("tx_grass.dds".into()), ..Default::default() };
        let mut w = Writer::new();
        external.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 12 + 1 + 4 + 12 + 4 + 4 + 4 + 1);
        assert_eq!(NiSourceTexture::read(&mut Reader::new(&w.out)).unwrap(), external);
        let internal =
            NiSourceTexture { source: TextureSource::Internal(Link::new(2)), ..Default::default() };
        let mut w = Writer::new();
        internal.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 12 + 1 + 1 + 4 + 4 + 4 + 4 + 1);
        assert_eq!(NiSourceTexture::read(&mut Reader::new(&w.out)).unwrap(), internal);
        let none = NiSourceTexture::default();
        let mut w = Writer::new();
        none.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 12 + 1 + 1 + 4 + 4 + 4 + 1);
        assert_eq!(NiSourceTexture::read(&mut Reader::new(&w.out)).unwrap(), none);
        // A file name that is cut short is refused, not read as an empty name.
        let mut bad = Writer::new();
        bad.u8(1);
        bad.u32(40);
        bad.bytes(b"short");
        let e = TextureSource::read(&mut Reader::new(&bad.out)).unwrap_err();
        assert!(e.what.contains("40 bytes wanted"), "{e}");
    }
}
