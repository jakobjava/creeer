//! What lies loose on the ground (G1.6c, ADR-0029): stones and fallen branches, from
//! `geology/loose.ron`. The ground is divided into cells on the cube-sphere's faces (about
//! `cell` metres a side), each surface point in one cell. What lies in a cell is made from the
//! planet's seed and the cell's key alone, so every machine makes the same, and nothing of it is
//! simulated or sent: it is scenery until a player picks something up (the server then makes
//! that one an item, and remembers it taken).
//!
//! A cell's stones are of the rock its ground is (or the bedrock beneath, `from_below`), with a
//! share of nodules of another; their number is a Poisson draw for the cell's area, their sizes
//! fragmentation's power law and their shapes a pebble's. Each lies flat, its shortest axis up,
//! on the ground as the terrain's density finds it (dug ground included).

use crate::items::{Form, Item, Piece};
use creeer_data::Data;
use creeer_math::glam::{DQuat, DVec3};
use creeer_terrain::Terrain;
use creeer_terrain::cube::CubeCoord;
use creeer_terrain::geology::rocks::RockTable;

/// Where the rules are kept in a pack.
pub const LOOSE: &str = "geology/loose.ron";

/// A cell: its face and its place on the face's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    pub face: u8,
    pub i: i32,
    pub j: i32,
}

/// What lies there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lying {
    /// A stone of a rock (its terrain material id), its edges (m) along its frame's axes.
    Stone { rock: u16, size: DVec3 },
    /// A branch along its frame's x.
    Branch {
        wood: usize,
        length: f64,
        radius: f64,
    },
}

/// One loose thing: its id (its cell and its place in it), its centre and its frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loose {
    pub id: u64,
    pub centre: DVec3,
    pub turn: DQuat,
    pub what: Lying,
}

/// The rules, resolved against a planet's rocks.
#[derive(Clone, Debug)]
pub struct Scatter {
    /// Cell side in face coordinates, and the smallest stone (m).
    step: f64,
    smallest: f64,
    /// By ground rock (terrain material id).
    stones: Vec<Option<StoneRule>>,
    branches: Vec<Option<BranchRule>>,
    /// The woods branches are of, by index (engineering material ids).
    pub woods: Vec<String>,
    seed: u64,
}

/// Bits for a cell's index on its face, and for a thing's in its cell.
const AXIS_BITS: u32 = 22;
const INDEX_BITS: u32 = 12;

impl Scatter {
    /// The packs' rules for `terrain`'s planet.
    ///
    /// # Errors
    /// If the file does not parse, names a rock the packs lack, or has a size or density that
    /// is not positive.
    pub fn from_data(data: &Data, terrain: &Terrain) -> Result<Self, String> {
        let text = data.file(LOOSE).map_err(|e| e.to_string())?;
        let def: LooseDef = ron::from_str(text).map_err(|e| format!("{LOOSE}: {e}"))?;
        let rocks = RockTable::from_data(data).map_err(|e| e.to_string())?;
        Self::new(&def, &rocks, terrain)
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

    /// The cells within `radius` (m) of point `p` (on a planet of `radius_planet`).
    pub fn cells_near(&self, p: DVec3, radius: f64) -> Vec<Cell> {
        let up = p.normalize();
        let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
        let across = radius / (self.step * p.length());
        #[allow(clippy::cast_possible_truncation)] // a few cells
        let n = across.ceil() as i32 + 1;
        let mut out: Vec<Cell> = Vec::new();
        for a in -n..=n {
            for b in -n..=n {
                let d = p + (east * f64::from(a) + north * f64::from(b)) * (radius / f64::from(n));
                let cell = self.cell_of(d);
                if !out.contains(&cell) {
                    out.push(cell);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// What lies in `cell`.
    pub fn in_cell(&self, terrain: &Terrain, cell: Cell) -> Vec<Loose> {
        let mut rng = Rng::new(self.seed ^ key(cell).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let corner = |i: i32, j: i32| {
            CubeCoord {
                face: cell.face,
                s: f64::from(i) * self.step,
                t: f64::from(j) * self.step,
            }
            .direction()
        };
        // Its middle, its area and its ground.
        let mid = CubeCoord {
            face: cell.face,
            s: (f64::from(cell.i) + 0.5) * self.step,
            t: (f64::from(cell.j) + 0.5) * self.step,
        };
        if mid.s.abs() > 1.0 || mid.t.abs() > 1.0 {
            return Vec::new();
        }
        let radius = terrain.planet().radius;
        let (a, b, c) = (
            corner(cell.i, cell.j),
            corner(cell.i + 1, cell.j),
            corner(cell.i, cell.j + 1),
        );
        let area = (b - a).cross(c - a).length() * radius * radius;
        let ground = on_ground(terrain, mid.direction());
        let up = ground.normalize();
        let Some(rock) = terrain.rock_at(ground - up * 0.3) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(rule) = &self.stones[usize::from(rock)] {
            let host = if rule.from_below {
                (1..=4).find_map(|k| {
                    let below = terrain.rock_at(ground - up * (3.0 * f64::from(k)))?;
                    (below != rock).then_some(below)
                })
            } else {
                Some(rock)
            };
            if let Some(host) = host {
                let host_rule = self.stones[usize::from(host)].as_ref().unwrap_or(rule);
                let n = rng.poisson(rule.per_m2 * area);
                for _ in 0..n {
                    let at = self.place(terrain, cell, &mut rng);
                    let mut stone = host;
                    let pick = rng.unit();
                    let mut sum = 0.0;
                    for &(nodule, share) in &host_rule.nodules {
                        sum += share;
                        if pick < sum {
                            stone = nodule;
                            break;
                        }
                    }
                    let d = self.smallest
                        / (1.0
                            - rng.unit()
                                * (1.0
                                    - (self.smallest / rule.largest)
                                        * (self.smallest / rule.largest)))
                            .sqrt();
                    let size = DVec3::new(
                        d,
                        d * (0.55 + 0.3 * rng.unit()),
                        d * (0.35 + 0.2 * rng.unit()),
                    );
                    let turn = lying(at, &mut rng);
                    let centre = at + at.normalize() * (size.z * 0.5);
                    out.push((centre, turn, Lying::Stone { rock: stone, size }));
                }
            }
        }
        if let Some(rule) = &self.branches[usize::from(rock)] {
            for _ in 0..rng.poisson(rule.per_m2 * area) {
                let at = self.place(terrain, cell, &mut rng);
                let length = rule.length.0 + rng.unit() * (rule.length.1 - rule.length.0);
                let radius = rule.radius.0 + rng.unit() * (rule.radius.1 - rule.radius.0);
                let turn = lying(at, &mut rng);
                let what = Lying::Branch {
                    wood: rule.wood,
                    length,
                    radius,
                };
                out.push((at + at.normalize() * radius, turn, what));
            }
        }
        (out.into_iter().enumerate())
            .filter(|(k, _)| *k < 1 << INDEX_BITS)
            .map(|(k, (centre, turn, what))| Loose {
                id: (key(cell) << INDEX_BITS) | k as u64,
                centre,
                turn,
                what,
            })
            .collect()
    }

    /// The loose thing `id`, if there is one.
    pub fn find(&self, terrain: &Terrain, id: u64) -> Option<Loose> {
        let k = id >> INDEX_BITS;
        let mask = (1u64 << AXIS_BITS) - 1;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // as packed
        let cell = Cell {
            face: (k >> (2 * AXIS_BITS)) as u8,
            i: ((k >> AXIS_BITS) & mask) as i32 - (1 << (AXIS_BITS - 1)),
            j: (k & mask) as i32 - (1 << (AXIS_BITS - 1)),
        };
        self.in_cell(terrain, cell).into_iter().find(|l| l.id == id)
    }

    /// A random point of `cell` on the ground.
    fn place(&self, terrain: &Terrain, cell: Cell, rng: &mut Rng) -> DVec3 {
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

    /// `loose` as an item, its materials' densities from `densities`.
    pub fn item(
        &self,
        loose: &Loose,
        rocks: &RockTable,
        densities: &crate::items::kit::Densities,
    ) -> Option<Item> {
        let piece = match loose.what {
            Lying::Stone { rock, size } => {
                let r = rocks.rocks.get(usize::from(rock))?;
                Piece::solid(&r.name, r.density, Form::Lump { size })
            }
            Lying::Branch {
                wood,
                length,
                radius,
            } => {
                let wood = self.woods.get(wood)?;
                Piece::solid(wood, densities.of(wood)?, Form::Rod { radius, length })
            }
        };
        Some(Item::Piece(piece, Vec::new()))
    }
}

/// A cell's key: its face and grid place, packed.
fn key(cell: Cell) -> u64 {
    let offset = |x: i32| (i64::from(x) + (1 << (AXIS_BITS - 1))) as u64 & ((1 << AXIS_BITS) - 1);
    (u64::from(cell.face) << (2 * AXIS_BITS)) | (offset(cell.i) << AXIS_BITS) | offset(cell.j)
}

/// The ground along direction `d`: from the heights, then onto the terrain's density
/// (edits included).
fn on_ground(terrain: &Terrain, d: DVec3) -> DVec3 {
    let mut p = d * (terrain.planet().radius + terrain.surface_height(d));
    for _ in 0..3 {
        p -= d * terrain.density(p);
    }
    p
}

/// A frame lying flat at `at` (its z up), turned at random about the vertical.
fn lying(at: DVec3, rng: &mut Rng) -> DQuat {
    let up = at.normalize();
    let flat = DQuat::from_rotation_arc(DVec3::Z, up);
    DQuat::from_axis_angle(up, rng.unit() * std::f64::consts::TAU) * flat
}

mod rng;
mod rules;
use rng::Rng;
use rules::{BranchRule, LooseDef, StoneRule};

#[cfg(test)]
mod tests;
