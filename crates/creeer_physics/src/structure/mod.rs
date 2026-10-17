//! Structures: anything built from members (frames, bridges, towers, vehicles, rockets).
//!
//! A structure is a set of nodes, each with a position and an orientation (6 degrees of
//! freedom), joined rigidly by beam elements ([`element`]). It is stepped in time by the
//! implicit solver of ADR-0004 ([`ImplicitSolver`]) or solved statically ([`statics`]).

use crate::gravity::PlanetGravity;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use rayon::prelude::*;

mod added;
mod assemble;
mod buckling;
#[cfg(test)]
mod buckling_tests;
mod builder;
mod coupled;
mod coupling;
mod dashpot;
#[cfg(test)]
mod dashpot_tests;
#[cfg(test)]
mod dynamics_tests;
mod element;
#[cfg(test)]
mod element_tests;
mod factor;
mod flow;
pub mod group;
#[cfg(test)]
mod hardening_tests;
mod heat;
#[cfg(test)]
mod heat_tests;
pub mod history;
mod lbfgs;
mod minimise;
mod objective;
mod plastic;
#[cfg(test)]
mod plastic_tests;
mod rigid_system;
#[cfg(test)]
mod rigid_tests;
mod save;
pub mod section;
mod settle;
pub mod shapes;
mod solver;
pub mod statics;
mod step;
#[cfg(test)]
mod validation_tests;

pub use added::AddedMass;
pub use builder::StructureBuilder;
pub use dashpot::Dashpot;
use element::Element;
pub use heat::BUILT_AT;
pub use plastic::Yielding;
pub(crate) use save::{read_solver, write_solver};
pub use section::{Material, RateSensitivity, Section};
pub use solver::{ImplicitSolver, Refactoring, Scheme, Timings};

/// Standard gravity on Earth (m/s²), pointing down (−y).
pub const STANDARD_GRAVITY: DVec3 = DVec3::new(0.0, -9.806_65, 0.0);

/// How a node is held in place. Supported axes are fixed in the world frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Support {
    /// Not held.
    Free,
    /// Position fixed, free to rotate (a pin or ball joint).
    Pinned,
    /// Position and orientation fixed (welded to the ground).
    Clamped,
    /// Per axis: 1 = free, 0 = fixed, for position and rotation (e.g. a roller).
    Custom {
        /// Free translation axes.
        translation: DVec3,
        /// Free rotation axes.
        rotation: DVec3,
    },
}

impl Support {
    fn masks(self) -> (DVec3, DVec3) {
        match self {
            Self::Free => (DVec3::ONE, DVec3::ONE),
            Self::Pinned => (DVec3::ZERO, DVec3::ONE),
            Self::Clamped => (DVec3::ZERO, DVec3::ZERO),
            Self::Custom {
                translation,
                rotation,
            } => (translation, rotation),
        }
    }
}

/// Index of a node in its structure.
pub type NodeId = usize;

/// The state and properties of one structure (struct-of-arrays over nodes).
#[derive(Clone, Debug)]
pub struct Structure {
    /// Node positions (m).
    pub position: Vec<DVec3>,
    /// Node orientations (rotation from the rest state).
    pub orientation: Vec<DQuat>,
    /// Node velocities (m/s).
    pub velocity: Vec<DVec3>,
    /// Node angular velocities (rad/s, world frame).
    pub angular_velocity: Vec<DVec3>,
    /// External forces on nodes (N), e.g. loads. Gravity is added separately.
    pub load: Vec<DVec3>,
    /// External torques on nodes (N·m).
    pub torque: Vec<DVec3>,
    /// Quadratic air drag per node, ½·ρ·C_d·A (kg/m): the force is −drag·|v|·v. Its
    /// coefficient uses the speed at the start of each step, which keeps it implicit
    /// (stable at any speed).
    pub drag: Vec<f64>,
    /// Loads that depend on the motion, linearised for the coming step ([`Dashpot`]): set
    /// with the loads they belong to, before each step (not saved).
    pub dashpots: Vec<Dashpot>,
    /// Water moving with nodes, for the coming step ([`AddedMass`]): set before each step
    /// (not saved).
    pub added_mass: Vec<AddedMass>,
    /// Uniform gravitational acceleration (m/s²).
    pub gravity: DVec3,
    /// A planet's gravity in its rotating frame (`gravity.rs`), on top of `gravity`.
    pub planet: Option<PlanetGravity>,
    pub(crate) rest: Vec<DVec3>,
    pub(crate) mass: Vec<f64>,
    /// Rotational inertia of each node in its rest frame (kg·m²).
    pub(crate) inertia: Vec<DMat3>,
    /// Free (1) or fixed (0) translation axes per node.
    pub(crate) free_translation: Vec<DVec3>,
    /// Free (1) or fixed (0) rotation axes per node.
    pub(crate) free_rotation: Vec<DVec3>,
    pub(crate) elements: Vec<Element>,
    /// Whether slender members are split when compressed, so they can buckle
    /// (`buckling.rs`; default on).
    pub splits: bool,
}

impl Structure {
    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.position.len()
    }

    /// Number of beam elements.
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Total mass (kg).
    pub fn total_mass(&self) -> f64 {
        self.mass.iter().sum()
    }

    /// Mass of each node (kg).
    pub fn masses(&self) -> &[f64] {
        &self.mass
    }

    /// Displacement of `node` from its rest position (m).
    pub fn displacement(&self, node: NodeId) -> DVec3 {
        self.position[node] - self.rest[node]
    }

    /// Whether any node is held by a support (then the structure cannot rotate as a whole).
    pub fn is_anchored(&self) -> bool {
        self.free_translation
            .iter()
            .chain(&self.free_rotation)
            .any(|m| *m != DVec3::ONE)
    }

    /// Largest axial strain |l/L − 1| over all members.
    pub fn max_axial_strain(&self) -> f64 {
        self.elements
            .iter()
            .map(|e| {
                let l = (self.position[e.b] - self.position[e.a]).length();
                (l / e.rest_length - 1.0).abs()
            })
            .fold(0.0, f64::max)
    }

    /// Total mechanical energy (J): kinetic + elastic + the potential of gravity and of the
    /// node loads (both relative to the rest shape). Without damping it stays constant.
    pub fn energy(&self) -> f64 {
        self.energy_and_activity().0
    }

    /// The total energy, and its kinetic + elastic part (a scale for how much is going on).
    pub(crate) fn energy_and_activity(&self) -> (f64, f64) {
        let (kinetic, potential) = self.node_energy();
        let active = kinetic + self.elastic_energy();
        (active + potential, active)
    }

    /// Kinetic energy and the potential of gravity and node loads (relative to rest).
    pub fn node_energy(&self) -> (f64, f64) {
        // Per node in parallel, summed in node order (deterministic).
        let parts: Vec<(f64, f64)> = (0..self.node_count())
            .into_par_iter()
            .with_min_len(256)
            .map(|i| {
                let r = DMat3::from_quat(self.orientation[i]);
                let inertia = r * self.inertia[i] * r.transpose();
                let w = self.angular_velocity[i];
                let kinetic = 0.5 * self.mass[i] * self.velocity[i].length_squared()
                    + 0.5 * w.dot(inertia * w);
                let mut potential = -(self.gravity * self.mass[i] + self.load[i])
                    .dot(self.position[i] - self.rest[i]);
                if let Some(planet) = &self.planet {
                    potential +=
                        self.mass[i] * planet.potential_difference(self.position[i], self.rest[i]);
                }
                (kinetic, potential)
            })
            .collect();
        parts
            .iter()
            .fold((0.0, 0.0), |(k, p), (dk, dp)| (k + dk, p + dp))
    }

    /// Elastic energy of all members (per-element work in parallel, summed in element order:
    /// deterministic).
    pub(crate) fn elastic_energy(&self) -> f64 {
        let rotations: Vec<DMat3> = self
            .orientation
            .par_iter()
            .map(|q| DMat3::from_quat(*q))
            .collect();
        let elastic: Vec<f64> = self
            .elements
            .par_iter()
            .with_min_len(128)
            .map(|e| {
                let (xa, xb) = (self.position[e.a], self.position[e.b]);
                let s = e.measures(xa, rotations[e.a], xb, rotations[e.b]);
                e.stiffness.energy(&s, &s)
            })
            .collect();
        elastic.iter().sum()
    }

    /// FNV-1a hash of the exact bits of the dynamic state, for determinism tests.
    pub fn state_hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let quats = self.orientation.iter().map(|q| q.to_array());
        let vectors = self
            .position
            .iter()
            .chain(&self.velocity)
            .chain(&self.angular_velocity)
            .map(|v| [v.x, v.y, v.z, 0.0]);
        for value in quats.chain(vectors).flatten() {
            for byte in value.to_bits().to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }
}
