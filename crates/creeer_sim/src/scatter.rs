//! What lies loose on the ground (G1.6c, ADR-0029): stones and fallen branches, from
//! `geology/loose.ron`. The ground is divided into cells on the cube-sphere's faces (about
//! `cell` metres a side, `cells.rs`), each surface point in one. What lies in a cell is made from
//! the planet's seed and the cell's key alone, so every machine makes the same, and nothing of it
//! is simulated or sent: it is scenery until a player picks something up (the server then makes
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
use creeer_terrain::geology::rocks::RockTable;

pub use cells::{Cell, Cells};

/// Where the rules are kept in a pack.
pub const LOOSE: &str = "geology/loose.ron";

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
    cells: Cells,
    /// The smallest stone (m).
    smallest: f64,
    /// By ground rock (terrain material id).
    stones: Vec<Option<StoneRule>>,
    branches: Vec<Option<BranchRule>>,
    /// The woods branches are of, by index (engineering material ids).
    pub woods: Vec<String>,
}

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
        self.cells.cell_of(d)
    }

    /// The cells within `radius` (m) of point `p` (`Cells::cells_near`).
    pub fn cells_near(&self, p: DVec3, radius: f64) -> Vec<Cell> {
        self.cells.cells_near(p, radius)
    }

    /// What lies in `cell`.
    pub fn in_cell(&self, terrain: &Terrain, cell: Cell) -> Vec<Loose> {
        let mut rng = self.cells.rng(cell, 0);
        let Some(mid) = self.cells.mid(cell) else {
            return Vec::new();
        };
        let area = self.cells.area(cell, terrain.planet().radius);
        let ground = cells::on_ground(terrain, mid.direction());
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
                    let at = self.cells.place(terrain, cell, &mut rng);
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
                let at = self.cells.place(terrain, cell, &mut rng);
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
        // (A cell at a face's edge reaches past it: what lies there is the next face's.)
        (out.into_iter())
            .filter(|(centre, ..)| self.cell_of(*centre) == cell)
            .enumerate()
            .filter(|(k, _)| *k < 1 << cells::INDEX_BITS)
            .map(|(k, (centre, turn, what))| Loose {
                id: (cells::key(cell) << cells::INDEX_BITS) | k as u64,
                centre,
                turn,
                what,
            })
            .collect()
    }

    /// The loose thing `id`, if there is one.
    pub fn find(&self, terrain: &Terrain, id: u64) -> Option<Loose> {
        (self.in_cell(terrain, cells::cell_of_id(id)).into_iter()).find(|l| l.id == id)
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

/// A frame lying flat at `at` (its z up), turned at random about the vertical.
fn lying(at: DVec3, rng: &mut Rng) -> DQuat {
    let up = at.normalize();
    let flat = DQuat::from_rotation_arc(DVec3::Z, up);
    DQuat::from_axis_angle(up, rng.unit() * std::f64::consts::TAU) * flat
}

pub(crate) mod cells;
mod rng;
mod rules;
pub(crate) use rng::Rng;
use rules::{BranchRule, LooseDef, StoneRule};

#[cfg(test)]
mod tests;
