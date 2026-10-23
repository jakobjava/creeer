//! Where trees stand (`docs/adr/0031-trees.md`, decision 3): cell by cell on the ground's grid
//! (`scatter::cells`), each species where its ranges allow (its latitude, the ground's height,
//! its slope and what the ground is), as many as its density draws (Poisson) for the cell's area,
//! their diameters a power law (fewer large than small), none within a metre and a half of
//! another in the cell. From the planet's seed and the cell alone: the same on every machine.

use super::species::Species;
use super::{Forest, Standing, TREE_CELL};
use crate::scatter::cells::{self, Cell, Cells};
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_terrain::Terrain;

/// What sets the forest's draws apart from the loose things' in the same cell.
const SALT: u64 = 0x7265_6573_5f6f_6f74;
/// Ids of trees are apart from the loose things' and the laid items'.
pub const TREE_ID: u64 = 1 << 62;
/// No two trees in a cell stand closer than this (m).
const APART: f64 = 1.5;

impl Forest {
    /// Every species, for `terrain`'s planet.
    pub fn new(species: Vec<Species>, terrain: &Terrain) -> Self {
        let planet = terrain.planet();
        Self {
            cells: Cells::new(TREE_CELL, planet.radius, planet.seed),
            species,
            clearing: None,
        }
    }

    /// The trees standing in `cell`.
    pub fn in_cell(&self, terrain: &Terrain, cell: Cell) -> Vec<Standing> {
        let mut rng = self.cells.rng(cell, SALT);
        let Some(mid) = self.cells.mid(cell) else {
            return Vec::new();
        };
        let up = mid.direction();
        // A cell wholly in the clearing has none (and needs no ground found).
        let cleared = |d: DVec3, margin: f64| {
            self.clearing
                .is_some_and(|(c, r)| (d * c.length() - c).length() + margin < r)
        };
        if cleared(up, TREE_CELL) {
            return Vec::new();
        }
        let height = terrain.surface_height(up);
        let ground = cells::on_ground(terrain, up);
        let Some(rock) = terrain.rock_at(ground - up * 0.3) else {
            return Vec::new();
        };
        let latitude = det::asin(up.y.clamp(-1.0, 1.0)).abs();
        let slope = slope(terrain, up);
        let area = self.cells.area(cell, terrain.planet().radius);
        let mut out: Vec<Standing> = Vec::new();
        for (s, species) in self.species.iter().enumerate() {
            let grows = (species.latitude.0..=species.latitude.1).contains(&latitude)
                && (species.elevation.0..=species.elevation.1).contains(&height)
                && slope <= species.slope
                && species.grounds.contains(&rock);
            // (Drawn whether it grows or not, so one species' range leaves the others' trees;
            // the ground found only for those that may stand.)
            let n = rng.poisson(species.per_m2 * area);
            for _ in 0..n {
                let at = self.cells.draw(cell, &mut rng);
                let dbh = size(species, rng.unit());
                let seed = rng.unit().to_bits();
                let yaw = rng.unit() * std::f64::consts::TAU;
                if !grows || cleared(at, 0.0) || self.cells.cell_of(at) != cell {
                    continue;
                }
                let base = cells::on_ground(terrain, at);
                if (out.iter()).any(|t| (t.base - base).length() < APART) {
                    continue;
                }
                let up = base.normalize();
                let turn = DQuat::from_axis_angle(up, yaw) * DQuat::from_rotation_arc(DVec3::Z, up);
                #[allow(clippy::cast_possible_truncation)] // a few species
                out.push(Standing {
                    id: 0,
                    base,
                    turn,
                    species: s as u16,
                    seed,
                    dbh,
                });
            }
        }
        for (k, t) in out.iter_mut().enumerate() {
            t.id = TREE_ID | (cells::key(cell) << cells::INDEX_BITS) | k as u64;
        }
        out
    }

    /// The tree `id`, if one stands there.
    pub fn find(&self, terrain: &Terrain, id: u64) -> Option<Standing> {
        let cell = cells::cell_of_id(id);
        self.in_cell(terrain, cell).into_iter().find(|t| t.id == id)
    }

    /// The cells within `radius` (m) of `p`.
    pub fn cells_near(&self, p: DVec3, radius: f64) -> Vec<Cell> {
        self.cells.cells_near(p, radius)
    }

    /// The cells about `p` within `radius` (m), sampled lightly (`Cells::cells_about`).
    pub fn cells_about(&self, p: DVec3, radius: f64) -> Vec<Cell> {
        self.cells.cells_about(p, radius)
    }
}

/// A diameter for `species` from uniform `u`: N(> d) ∝ d^(−sizes) between its least and most.
fn size(species: &Species, u: f64) -> f64 {
    let (lo, hi) = species.dbh;
    let k = species.sizes;
    let (a, b) = (det::pow(lo, -k), det::pow(hi, -k));
    det::pow(a - u * (a - b), -1.0 / k)
}

/// The ground's slope at direction `up` (rad): the steepest of two crossing differences 4 m
/// long.
fn slope(terrain: &Terrain, up: DVec3) -> f64 {
    let r = terrain.planet().radius;
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let h = |d: DVec3| terrain.surface_height((up * r + d * 2.0).normalize());
    let gx = (h(east) - h(-east)) / 4.0;
    let gy = (h(north) - h(-north)) / 4.0;
    det::atan((gx * gx + gy * gy).sqrt())
}
