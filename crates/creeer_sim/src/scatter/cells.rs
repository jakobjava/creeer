//! The ground's cells (G1.6c, ADR-0029; shared with the forest, G1.7): squares on the
//! cube-sphere's faces, about `cell` metres a side, each surface point in one. What lies or
//! grows in a cell is made from the planet's seed and the cell's key alone, so every machine
//! makes the same.

use super::Rng;
use creeer_math::glam::DVec3;
use creeer_terrain::Terrain;
use creeer_terrain::cube::CubeCoord;

/// A cell: its face and its place on the face's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    pub face: u8,
    pub i: i32,
    pub j: i32,
}

/// Bits for a cell's index on its face, and for a thing's in its cell.
const AXIS_BITS: u32 = 22;
pub(crate) const INDEX_BITS: u32 = 12;

/// The grid of cells on a planet.
#[derive(Clone, Debug)]
pub struct Cells {
    /// Cell side in face coordinates.
    step: f64,
    pub(crate) seed: u64,
}

impl Cells {
    /// Cells about `cell` metres a side on a planet of `radius` (m) and `seed`.
    pub fn new(cell: f64, radius: f64, seed: u64) -> Self {
        Self {
            step: cell / radius,
            seed,
        }
    }

    /// The cell direction `d` lies in.
    pub fn cell_of(&self, d: DVec3) -> Cell {
        let c = CubeCoord::from_direction(d);
        #[allow(clippy::cast_possible_truncation)] // within the face: a few hundred thousand
        let index = |s: f64| (s / self.step).floor() as i32;
        Cell {
            face: c.face,
            i: index(c.s),
            j: index(c.t),
        }
    }

    /// The cells within `radius` (m) of point `p`: every cell some point of the disc about `p`
    /// lies in. The disc is sampled at a third of the smallest cell (the warp makes a cell
    /// 1/1.35 of `cell` across at the least, between a face's middle and its edge), so none is
    /// missed (G1.6c: sampled at a cell's size, cells were skipped, and stones came and went as
    /// the camera moved).
    pub fn cells_near(&self, p: DVec3, radius: f64) -> Vec<Cell> {
        let up = p.normalize();
        let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
        let spacing = self.step * p.length() / 1.35 / 3.0;
        #[allow(clippy::cast_possible_truncation)] // a few dozen
        let n = (radius / spacing).ceil() as i32;
        let mut out = std::collections::BTreeSet::new();
        for a in -n..=n {
            for b in -n..=n {
                let off = (east * f64::from(a) + north * f64::from(b)) * spacing;
                if off.length() <= radius + spacing {
                    out.insert(self.cell_of(p + off));
                }
            }
        }
        out.into_iter().collect()
    }

    /// The generator for what `cell` holds, apart for each `salt` (0: the loose things').
    pub(crate) fn rng(&self, cell: Cell, salt: u64) -> Rng {
        Rng::new(self.seed ^ salt ^ key(cell).wrapping_mul(0x9e37_79b9_7f4a_7c15))
    }

    /// `cell`'s middle, if it lies on its face.
    pub(crate) fn mid(&self, cell: Cell) -> Option<CubeCoord> {
        let mid = CubeCoord {
            face: cell.face,
            s: (f64::from(cell.i) + 0.5) * self.step,
            t: (f64::from(cell.j) + 0.5) * self.step,
        };
        (mid.s.abs() <= 1.0 && mid.t.abs() <= 1.0).then_some(mid)
    }

    /// `cell`'s area (m²) on a sphere of `radius`.
    pub(crate) fn area(&self, cell: Cell, radius: f64) -> f64 {
        let corner = |i: i32, j: i32| {
            CubeCoord {
                face: cell.face,
                s: f64::from(i) * self.step,
                t: f64::from(j) * self.step,
            }
            .direction()
        };
        let (a, b, c) = (
            corner(cell.i, cell.j),
            corner(cell.i + 1, cell.j),
            corner(cell.i, cell.j + 1),
        );
        (b - a).cross(c - a).length() * radius * radius
    }

    /// A random point of `cell` on the ground.
    pub(crate) fn place(&self, terrain: &Terrain, cell: Cell, rng: &mut Rng) -> DVec3 {
        let s = (f64::from(cell.i) + rng.unit()) * self.step;
        let t = (f64::from(cell.j) + rng.unit()) * self.step;
        on_ground(
            terrain,
            CubeCoord {
                face: cell.face,
                s,
                t,
            }
            .direction(),
        )
    }
}

/// A cell's key: its face and grid place, packed.
pub(crate) fn key(cell: Cell) -> u64 {
    let offset = |x: i32| (i64::from(x) + (1 << (AXIS_BITS - 1))) as u64 & ((1 << AXIS_BITS) - 1);
    (u64::from(cell.face) << (2 * AXIS_BITS)) | (offset(cell.i) << AXIS_BITS) | offset(cell.j)
}

/// The cell a thing's id (`key << INDEX_BITS | index`, below `high` flag bits) is in.
pub(crate) fn cell_of_id(id: u64) -> Cell {
    let k = (id & ((1u64 << (2 * AXIS_BITS + 3 + INDEX_BITS)) - 1)) >> INDEX_BITS;
    let mask = (1u64 << AXIS_BITS) - 1;
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // as packed
    Cell {
        face: (k >> (2 * AXIS_BITS)) as u8,
        i: ((k >> AXIS_BITS) & mask) as i32 - (1 << (AXIS_BITS - 1)),
        j: (k & mask) as i32 - (1 << (AXIS_BITS - 1)),
    }
}

/// The ground along direction `d`: from the heights, then onto the terrain's density
/// (edits included).
pub(crate) fn on_ground(terrain: &Terrain, d: DVec3) -> DVec3 {
    let mut p = d * (terrain.planet().radius + terrain.surface_height(d));
    for _ in 0..3 {
        p -= d * terrain.density(p);
    }
    p
}
