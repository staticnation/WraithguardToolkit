//! What the ground is doing at a point — round 10.
//!
//! The scatter has always known *what is here*: which texture, how steep, how high,
//! whether an object is in the way. It never asked what the ground is *doing* — which way
//! the slope faces, whether the point sits in a hollow or on a ridge, how far the sea is —
//! though every one of those is in data it already held. This module reads them.
//!
//! **Across cell borders.** Heights sit on a 65×65 grid per cell, and curvature and the
//! shoreline both look some hundreds of units out from the point. A reader that clamped to
//! the home cell would see flat ground along every cell edge — a 128-unit band on a grid,
//! which on a map is the most visible seam there is. So every sample goes through the
//! world's own table of landscapes and reads the neighbour where the point crosses; only
//! at the edge of the world, where there is no neighbour, does it fall back to the home
//! cell's clamped answer.
//!
//! Everything here is a pure function of position. No random numbers, which is what lets
//! the guarded nudge (`poisson::place`) ask these questions a second time at a moved spot
//! without disturbing anything else.

use crate::land::{Land, CELL_SIZE};
use crate::world::World;

/// The ground around one cell, read through the world so a sample may cross an edge.
pub struct Ground<'a> {
    world: &'a World,
    home: &'a Land,
    origin: [f32; 2],
    /* Round 18bo: the eight neighbours, looked up once.
     *
     * Every height off the home cell used to be a `HashMap` probe — `world.lands.get(&g)`
     * — and the scatter takes millions of them: callgrind counted 3.2 million SipHash
     * rounds in one export, 2.4% of the run, all of them asking the same nine questions
     * over and over. A cell's samples reach one cell out at most (the pad is a fraction of
     * a cell, and the widest finite difference is 64 units), so nine answers cover them.
     *
     * Indexed `[dy + 1][dx + 1]` from the home cell. The map is still there for anything
     * that somehow reaches further, so this is a cache and not a new rule about range. */
    near: [[Option<&'a Land>; 3]; 3],
}

/// Step for the finite differences, in units. Half a vertex: the field is piecewise
/// linear on 128-unit triangles, so anything finer measures the same plane twice.
const STEP: f32 = 64.0;

/// Beyond this the first-order shoreline estimate has stopped meaning anything, and no
/// band anybody writes is this wide. One cell.
const FAR: f32 = CELL_SIZE;

impl<'a> Ground<'a> {
    pub fn new(world: &'a World, home: &'a Land) -> Self {
        let (gx, gy) = home.grid;
        let mut near: [[Option<&'a Land>; 3]; 3] = [[None; 3]; 3];
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                near[(dy + 1) as usize][(dx + 1) as usize] = world.lands.get(&(gx + dx, gy + dy));
            }
        }
        Ground { world, home, origin: [gx as f32 * CELL_SIZE, gy as f32 * CELL_SIZE], near }
    }

    /// The landscape holding cell `g`, from the nine cached neighbours where it is one of
    /// them and from the world's table otherwise. Exactly what `world.lands.get(&g)`
    /// answers, either way.
    #[inline]
    fn land_at(&self, g: (i32, i32)) -> Option<&'a Land> {
        let (dx, dy) = (g.0 - self.home.grid.0, g.1 - self.home.grid.1);
        if (-1..=1).contains(&dx) && (-1..=1).contains(&dy) {
            return self.near[(dy + 1) as usize][(dx + 1) as usize];
        }
        self.world.lands.get(&g)
    }

    /// Height at a world position, from whichever cell holds it.
    ///
    /// Off the edge of the world — no landscape record there — the home cell's answer at
    /// the clamped position stands in, so a difference taken across that edge reads as
    /// flat rather than as a cliff down to nothing.
    #[inline]
    pub fn height(&self, wx: f32, wy: f32) -> f32 {
        let g = (crate::land::floor_i32(wx / CELL_SIZE), crate::land::floor_i32(wy / CELL_SIZE));
        if g == self.home.grid {
            return self.home.height_at(wx - self.origin[0], wy - self.origin[1]);
        }
        match self.land_at(g) {
            Some(l) => l.height_at(wx - g.0 as f32 * CELL_SIZE, wy - g.1 as f32 * CELL_SIZE),
            None => self.home.height_at(
                (wx - self.origin[0]).clamp(0.0, CELL_SIZE - 0.001),
                (wy - self.origin[1]).clamp(0.0, CELL_SIZE - 0.001),
            ),
        }
    }

    /// Hollow or ridge: how far the ground here stands above the average of the ground
    /// `radius` units away in the four compass directions, in units.
    ///
    /// Positive is a ridge (the centre is higher than its surroundings), negative a
    /// hollow. Four taps rather than a ring of eight: at the radii this is used at the
    /// two agree to within the noise of the height field, and every tap is a triangle
    /// walk through the world's landscape table.
    pub fn curvature(&self, wx: f32, wy: f32, radius: f32) -> f32 {
        let r = radius.max(1.0);
        let around = (self.height(wx + r, wy)
            + self.height(wx - r, wy)
            + self.height(wx, wy + r)
            + self.height(wx, wy - r))
            * 0.25;
        self.height(wx, wy) - around
    }

    /// Signed horizontal distance to where the land meets water at `water`, in units:
    /// positive on land, negative under water.
    ///
    /// The waterline is the contour where height crosses the water level, so
    /// `(h − water) / |∇h|` is the distance to it to first order — the same trick the ban
    /// fade uses to find the edge of a banned texture, and exact on planar ground. It
    /// degrades far from the shore, which does not matter when the question is "within N
    /// units", and it is clamped to [`FAR`] both ways so a band can never accidentally
    /// mean "everywhere". On ground flat enough that the gradient vanishes the answer is
    /// simply far, on whichever side of the water the point is.
    pub fn shore(&self, wx: f32, wy: f32, water: f32) -> f32 {
        let h = self.height(wx, wy) - water;
        let gx = (self.height(wx + STEP, wy) - self.height(wx - STEP, wy)) / (2.0 * STEP);
        let gy = (self.height(wx, wy + STEP) - self.height(wx, wy - STEP)) / (2.0 * STEP);
        let g = (gx * gx + gy * gy).sqrt();
        if g < 1e-4 {
            return if h >= 0.0 { FAR } else { -FAR };
        }
        (h / g).clamp(-FAR, FAR)
    }

    /// Round 18k: the yaw that lays a mesh along the shore at this spot, in radians -
    /// its X axis along the waterline, its +Y facing the water - or `None` on ground so
    /// flat there is no downhill to speak of, where the caller keeps the random spin.
    ///
    /// The waterline is the contour, and a contour runs at right angles to the
    /// gradient; downhill is `−∇h`. Under a yaw θ (`poisson::ref_angles`) the mesh's X
    /// axis points along (cos θ, sin θ) and its Y along (−sin θ, cos θ), so Y = downhill
    /// = (−gx, −gy)/|g| gives sin θ = gx/|g| and cos θ = −gy/|g|.
    pub fn shore_heading(&self, wx: f32, wy: f32) -> Option<f32> {
        let gx = (self.height(wx + STEP, wy) - self.height(wx - STEP, wy)) / (2.0 * STEP);
        let gy = (self.height(wx, wy + STEP) - self.height(wx, wy - STEP)) / (2.0 * STEP);
        let g = (gx * gx + gy * gy).sqrt();
        if g < 1e-4 {
            return None;
        }
        Some((gx / g).atan2(-gy / g))
    }
}

/// Which way a surface faces, in compass degrees: 0 north (+Y), 90 east (+X), as on a
/// map. A slope faces the direction its normal leans toward.
///
/// Meaningless on flat ground — the horizontal part of the normal is zero and the angle
/// is noise — which is why the facing gate is skipped below the rule's shallowest slope
/// rather than answered here. Given a normal with no lean at all this returns 0, which
/// callers must not read as "north".
#[inline]
pub fn facing_deg(n: [f32; 3]) -> f32 {
    if n[0].abs() < 1e-6 && n[1].abs() < 1e-6 {
        return 0.0;
    }
    n[0].atan2(n[1]).to_degrees().rem_euclid(360.0)
}

/// The slope of a surface with normal `n`, in degrees from level.
#[inline]
pub fn slope_deg(n: [f32; 3]) -> f32 {
    n[2].clamp(-1.0, 1.0).acos().to_degrees()
}
