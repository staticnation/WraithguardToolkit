//! `PGRD`: a cell's path grid, the points NPCs walk between and their connections. `tes3`
//! pathgrid.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathGrid {
    pub flags: u32,
    /// `NAME`: the cell the grid belongs to, empty for an exterior.
    pub cell: String,
    pub data: PathGridData,
    pub points: Vec<PathGridPoint>,
    /// `PGRC`: point indices, each point's `connection_count` of them in turn.
    pub connections: Vec<u32>,
}

/// `DATA`, 12 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathGridData {
    pub grid: (i32, i32),
    pub granularity: u16,
    pub point_count: u16,
}

impl PathGridData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<PathGridData> {
        Ok(PathGridData { grid: (r.i32()?, r.i32()?), granularity: r.u16()?, point_count: r.u16()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32(self.grid.0);
        w.i32(self.grid.1);
        w.u16(self.granularity);
        w.u16(self.point_count);
    }
}

/// One `PGRP` entry, 16 bytes: a position, two flag bytes and two padding bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathGridPoint {
    pub location: [i32; 3],
    pub auto_generated: u8,
    pub connection_count: u8,
}

impl PathGridPoint {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<PathGridPoint> {
        let location = r.i32s::<3>()?;
        let auto_generated = r.u8()?;
        let connection_count = r.u8()?;
        r.skip(2)?;
        Ok(PathGridPoint { location, auto_generated, connection_count })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32s(&self.location);
        w.u8(self.auto_generated);
        w.u8(self.connection_count);
        w.bytes(&[0; 2]);
    }
}

impl Typed for PathGrid {
    const TAG: Tag = *b"PGRD";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.cell = text(s),
                b"DATA" => this.data = PathGridData::read(&mut sized(rec, s, PathGridData::SIZE)?)?,
                b"PGRP" => {
                    // As many whole points as the subrecord holds; tes3 drops a trailing
                    // part of one the same way.
                    let mut r = reader(s);
                    this.points = Vec::with_capacity(s.data.len() / PathGridPoint::SIZE);
                    for _ in 0..s.data.len() / PathGridPoint::SIZE {
                        this.points.push(PathGridPoint::read(&mut r)?);
                    }
                }
                b"PGRC" => {
                    let mut r = reader(s);
                    this.connections = Vec::with_capacity(s.data.len() / 4);
                    for _ in 0..s.data.len() / 4 {
                        this.connections.push(r.u32()?);
                    }
                }
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.sub(b"DATA", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text(b"NAME", &self.cell)?;
        if !self.points.is_empty() {
            out.sub(b"PGRP", |w| {
                for p in &self.points {
                    p.write(w);
                }
                Ok(())
            })?;
        }
        if !self.connections.is_empty() {
            out.sub(b"PGRC", |w| {
                w.u32s(&self.connections);
                Ok(())
            })?;
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_grid_round_trips_and_a_short_data_is_refused() {
        let mut rec = Record::new(b"PGRD");
        let mut w = Writer::new();
        PathGridData { grid: (3, -4), granularity: 1024, point_count: 2 }.write(&mut w);
        rec.push(b"DATA", w.into_bytes());
        rec.push(b"NAME", b"\0".to_vec());
        let mut w = Writer::new();
        PathGridPoint { location: [100, 200, 30], auto_generated: 1, connection_count: 1 }.write(&mut w);
        PathGridPoint { location: [-100, 50, 31], auto_generated: 0, connection_count: 1 }.write(&mut w);
        rec.push(b"PGRP", w.into_bytes());
        let mut w = Writer::new();
        w.u32s(&[1, 0]);
        rec.push(b"PGRC", w.into_bytes());
        let p = PathGrid::from_raw(&rec).unwrap();
        assert_eq!(p.cell, "");
        assert_eq!(p.data.grid, (3, -4));
        assert_eq!(p.points.len(), 2);
        assert_eq!(p.points[1].location, [-100, 50, 31]);
        assert_eq!(p.connections, vec![1, 0]);
        assert_eq!(p.to_raw().unwrap(), rec);

        // An empty grid writes DATA and NAME only, in that order.
        let empty = PathGrid { cell: "Arrille's Tradehouse".into(), ..Default::default() };
        let tags: Vec<Tag> = empty.to_raw().unwrap().subs.iter().map(|s| s.tag).collect();
        assert_eq!(tags, vec![*b"DATA", *b"NAME"]);

        rec.subs[0].data.to_mut().truncate(10);
        let e = PathGrid::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("PGRD::DATA is 10 bytes"), "{e}");
    }
}
