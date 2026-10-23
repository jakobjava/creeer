//! Trees (G1.7, `docs/adr/0031-trees.md`): one generator turns a species' attributes
//! (`flora/*.ron`) into trees, so a new tree is a data entry, not code.
//!
//! - `species.rs`: the attributes, resolved and checked.
//! - `grow.rs`: a tree from its species, its diameter and a seed: the stem's taper, the branches
//!   on their nodes, the crown's biomass shared among them.
//! - `place.rs`: where trees stand, cell by cell, as the loose things lie: from the planet's seed,
//!   the same on every machine, and nothing simulated: scenery until struck.
//! - `section.rs`, `chop.rs`: a cut into a stem, the wood each blow takes out of it, and whether
//!   what is left carries the tree.
//! - `felled.rs`: a felled tree's parts as rigid bodies, and a standing trunk's boxes.

use creeer_data::Data;
use creeer_math::glam::{DQuat, DVec2, DVec3};
use creeer_terrain::Terrain;

pub mod chop;
pub mod felled;
pub mod grow;
mod place;
pub mod section;
pub mod species;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cut;

pub use chop::Cut;
pub use felled::Part;
pub use grow::Tree;
pub use place::TREE_ID;
pub use species::Species;

/// Where the species are kept in the packs (their lists merged by name).
pub const TREES: &str = "flora/*.ron";
/// A cell of the forest's grid (m): the loose things'.
pub const TREE_CELL: f64 = 8.0;
/// Standard gravity (m/s²) for a tree's own weight.
const G: f64 = 9.806_65;
/// A crown's drag coefficient, on its outline's area: Sitka spruce's, streamlined (Mayhead;
/// in-situ Norway spruce 0.59 at 30 m/s, broadleaves 0.15–0.22 at 20 m/s, Vollsinger et al.
/// 2005).
const CROWN_DRAG: f64 = 0.35;
/// Air's density (kg/m³) for the wind on a crown.
const AIR: f64 = 1.2;
/// Heights a stem is checked at under its own weight (m apart).
const CHECKED_EVERY: f64 = 0.5;

/// The species of the packs on a planet's grid of cells.
#[derive(Clone, Debug)]
pub struct Forest {
    cells: crate::scatter::Cells,
    pub species: Vec<Species>,
    /// A clearing no tree stands in: its centre and radius (m).
    clearing: Option<(DVec3, f64)>,
}

/// A tree standing: its id, its base, its frame (+z up its stem), and what grows it again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Standing {
    pub id: u64,
    pub base: DVec3,
    pub turn: DQuat,
    pub species: u16,
    pub seed: u64,
    pub dbh: f64,
}

impl Forest {
    /// The packs' species, for `terrain`'s planet.
    ///
    /// # Errors
    /// If a species does not load (`species.rs`).
    pub fn from_data(data: &Data, terrain: &Terrain) -> Result<Self, String> {
        Ok(Self::new(species::load(data)?, terrain))
    }

    /// The same forest with a clearing of `radius` (m) about `centre`, where no tree stands (the
    /// spawn's: a player starts in the open).
    #[must_use]
    pub fn with_clearing(mut self, centre: DVec3, radius: f64) -> Self {
        self.clearing = Some((centre, radius));
        self
    }

    /// `standing`, grown.
    pub fn grow(&self, standing: &Standing) -> Tree {
        Tree::grow(
            &self.species[usize::from(standing.species)],
            standing.seed,
            standing.dbh,
        )
    }

    /// The tree a part was cut from, grown again.
    pub fn grow_part(&self, part: &Part) -> Option<Tree> {
        let species = self.species.get(usize::from(part.species))?;
        Some(Tree::grow(species, part.seed, part.dbh))
    }
}

impl Tree {
    /// The wind's force on its crown (N, tree frame) in wind `wind` (m/s), and the height it
    /// acts at (m): drag on the crown's outline.
    pub fn wind_on_crown(&self, species: &Species, wind: DVec3) -> (DVec3, f64) {
        let length = self.height - self.crown_base;
        let widest = species.crown.radius.0 + species.crown.radius.1 * self.dbh;
        let outline = match species.crown.outline {
            species::Outline::Cone => 1.0,
            species::Outline::Paraboloid => 4.0 / 3.0,
            species::Outline::Ellipsoid => std::f64::consts::FRAC_PI_2,
        };
        let flat = DVec3::new(wind.x, wind.y, 0.0);
        let force = flat * (0.5 * AIR * CROWN_DRAG * outline * length * widest * flat.length());
        (force, self.crown_base + 0.5 * length)
    }

    /// Where the load on a section at height `z` bears (tree frame, in its plane), and how much
    /// it presses (N): the weight above, its line moved by the wind's moment.
    fn load_at(&self, species: &Species, z: f64, wind: DVec3) -> (f64, DVec2) {
        let (mass, centre) = self.above(z);
        let weight = mass * G;
        let (force, at) = self.wind_on_crown(species, wind);
        let shift = force.truncate() * ((at - z).max(0.0) / weight.max(f64::MIN_POSITIVE));
        (weight, centre.truncate() + shift)
    }

    /// The most any part of it is used (≥ 1 breaks) standing under its own weight and `wind`
    /// (m/s, tree frame): its stem at every half metre, each branch at its base.
    pub fn self_weight(&self, species: &Species, wind: DVec3) -> f64 {
        let mut most = 0.0f64;
        let mut z = 0.0;
        while z < self.height - CHECKED_EVERY {
            let (weight, at) = self.load_at(species, z, wind);
            if let Some(s) = section::Section::cut(self.radius(z), &[]) {
                most = most.max(s.utilisation(weight, at, species.bending, species.compression));
            }
            z += CHECKED_EVERY;
        }
        for node in &self.nodes {
            for b in &node.branches {
                let out = creeer_math::det::sin(b.angle) * b.length;
                let moment = G * (b.wood * 0.25 + b.foliage * 0.5) * out;
                let modulus = std::f64::consts::FRAC_PI_4 * b.radius * b.radius * b.radius;
                most = most.max(moment / modulus / species.bending);
            }
        }
        most
    }

    /// How a cut into its stem is used (≥ 1: it goes over) under its weight and `wind`, the point
    /// (tree frame, in the cut's plane) it pivots on as it goes (its hinge's middle) and the way
    /// it goes (unit); none if nothing is left.
    pub fn at_cut(&self, species: &Species, cut: &Cut, wind: DVec3) -> Option<(f64, DVec2, DVec2)> {
        let section = section::Section::cut(self.radius(cut.z), &cut.notches(self.radius(cut.z)))?;
        let (weight, at) = self.load_at(species, cut.z, wind);
        let used = section.utilisation(weight, at, species.bending, species.compression);
        let (pivot, way) = section.hinge(at);
        Some((used, pivot, way))
    }
}
