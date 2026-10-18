//! Procedural solar systems (ADR-0007): a star, the hand-made home planet on its given
//! orbit, further rocky planets and gas giants spaced by mutual Hill radii, moons between
//! their planet's Roche limit and a fraction of its Hill sphere (tidally locked), an asteroid
//! belt with a few bodies massive enough to matter, and Trojans at the giants' L4 and L5.
//!
//! What may be generated is data (`data/systems/*.ron`, [`def`]); the same seed gives the
//! same system on every platform. Each candidate is integrated for centuries
//! ([`stability`]) and replaced by the next seed's candidate if anything leaves its orbit.

mod build;
pub mod def;
mod names;
pub mod rng;
mod stability;
#[cfg(test)]
mod tests;

use crate::body::Body;
use crate::ephemeris::Ephemeris;
use crate::rails::{Group, Particle};
use creeer_data::Data;
use creeer_math::glam::DVec3;
pub use def::{DefError, Params};
use rng::Rng;

/// A body with its start state (inertial, barycentric, at t = 0).
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The body.
    pub body: Body,
    /// Position (m).
    pub x: DVec3,
    /// Velocity (m/s).
    pub v: DVec3,
}

/// A generated system.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    /// Name of the system.
    pub name: String,
    /// Bodies that pull on each other: star first.
    pub majors: Vec<Placed>,
    /// Small bodies on rails (belt asteroids, Trojans).
    pub minors: Vec<Placed>,
    /// For every major, the body it was placed around.
    pub parents: Vec<Option<usize>>,
    /// Index of the home planet among the majors.
    pub home: usize,
    /// Candidates generated before this one passed the checks (0 = the first did).
    pub attempts: u32,
}

/// Why no system could be generated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerateError {
    /// The definition file is wrong.
    Definition(DefError),
    /// Every attempt failed a stability check; the last reason.
    Unstable(String),
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definition(e) => write!(f, "{e}"),
            Self::Unstable(e) => write!(f, "no stable system found: {e}"),
        }
    }
}

impl std::error::Error for GenerateError {}

/// Generates the system a definition describes.
pub fn generate(params: &Params) -> Result<Generated, GenerateError> {
    let mut last = String::new();
    for attempt in 0..params.stability.attempts.max(1) {
        let mut rng = Rng::new(params.seed, u64::from(attempt));
        let mut system = build::build(params, &mut rng);
        system.attempts = attempt;
        let (planets, moons) = rayon::join(
            || stability::planets(&system, &params.stability),
            || stability::moons(&system, &params.stability),
        );
        match planets.and(moons) {
            Ok(()) => return Ok(system),
            Err(reason) => last = reason,
        }
    }
    Err(GenerateError::Unstable(last))
}

/// Loads a definition file and generates its system.
pub fn generate_file(data: &Data, path: &str) -> Result<Generated, GenerateError> {
    let params = Params::load(data, path).map_err(GenerateError::Definition)?;
    generate(&params)
}

impl Generated {
    /// The majors as an ephemeris starting at t = 0.
    pub fn ephemeris(&self) -> Ephemeris {
        Ephemeris::new(
            self.majors.iter().map(|m| m.body.clone()).collect(),
            self.majors.iter().map(|m| m.x).collect(),
            self.majors.iter().map(|m| m.v).collect(),
            0.0,
        )
    }

    /// Every minor body as its own rails group (starting at t = 0).
    pub fn minor_groups(&self) -> Vec<Group> {
        self.minors
            .iter()
            .map(|m| {
                Group::new(
                    0.0,
                    &[(m.x, m.v)],
                    vec![Particle {
                        mass: m.body.mass(),
                        mu: m.body.mu,
                    }],
                )
            })
            .collect()
    }

    /// A hash of every start state and body property (golden tests: the same seed must give
    /// the same system on every platform).
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bits: u64| {
            h ^= bits;
            h = h.wrapping_mul(0x0100_0000_01b3);
        };
        for p in self.majors.iter().chain(&self.minors) {
            for value in [
                p.x.x,
                p.x.y,
                p.x.z,
                p.v.x,
                p.v.y,
                p.v.z,
                p.body.mu,
                p.body.radius,
            ] {
                eat(value.to_bits());
            }
            let q = p.body.spin.orientation;
            for value in [q.x, q.y, q.z, q.w, p.body.spin.rate] {
                eat(value.to_bits());
            }
            for byte in p.body.name.bytes() {
                eat(u64::from(byte));
            }
        }
        h
    }
}
