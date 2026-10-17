//! Water on bodies (F6.6, ADR-0014): a planet's ocean fills every basin up to sea level, the
//! equipotential of its rotating field ([`creeer_terrain::datum`]); a flat world's lies at
//! y = 0. Near a body the sea is the plane tangent to that surface.
//!
//! - **Buoyancy:** the weight of the water displaced, −ρ·V·g at the centre of buoyancy, from
//!   the exact submerged volume of each shape ([`volume`]): Archimedes, the hydrostatic
//!   pressure ρ·g·depth integrated over the hull.
//! - **Added mass:** half the displaced water moves with the body (a sphere's exactly; the
//!   rest's taken alike), as inertia without weight ([`creeer_physics::structure::AddedMass`]):
//!   water a body enters is at rest, water it leaves keeps its motion.
//! - **Drag:** −½·ρ·C_d·A·|v|·v on the submerged part's centre, A the area it shows the flow
//!   (its wetted faces projected), and a turning body's −½·ρ·C_d·V^(5/3)·|ω|·ω, as a dashpot:
//!   hull drag, crudely. Waves, and the damping of the waves a heaving or rolling hull makes,
//!   are a later phase's: without them a floating body bobs long, its drag falling with its
//!   speed. Fins, keels and rudders are aero surfaces in
//!   water ([`crate::aero`]).

pub mod flood;
#[cfg(test)]
mod tests;
pub mod volume;

use creeer_fluids::atmosphere::Air;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::{Dashpot, NodeId};
use creeer_physics::world::World;
use creeer_physics::world::shape::Shape;
use creeer_terrain::OceanData;
use creeer_terrain::datum::Datum;
use volume::Waterline;

/// Added mass per volume of water displaced (a sphere's, ½).
pub const ADDED: f64 = 0.5;

/// Where the sea's surface is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    /// A flat world: y = 0, up +y.
    Flat,
    /// A planet's datum (centred at the origin).
    Planet(Datum),
}

/// The sea.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sea {
    pub level: Level,
    pub water: OceanData,
    /// The pressure on its surface (Pa: the atmosphere's there).
    pub surface_pressure: f64,
}

impl Sea {
    /// The surface near `p`: a point of it and its upward normal.
    pub fn surface(&self, p: DVec3) -> (DVec3, DVec3) {
        match self.level {
            Level::Flat => (DVec3::new(p.x, 0.0, p.z), DVec3::Y),
            Level::Planet(datum) => {
                // Down the level's normal to it (three steps: the normal turns 10⁻⁷ rad a km).
                let mut q = p;
                for _ in 0..3 {
                    let (r, gradient) = datum.radius_gradient(q);
                    let up = (q.normalize() - gradient).normalize();
                    q -= up * ((q.length() - r) / up.dot(q.normalize()));
                }
                let (_, gradient) = datum.radius_gradient(q);
                (q, (q.normalize() - gradient).normalize())
            }
        }
    }

    /// How deep `p` is (m; negative: above the sea).
    pub fn depth(&self, p: DVec3) -> f64 {
        let (at, up) = self.surface(p);
        (at - p).dot(up)
    }

    /// The water at `p` as a medium for surfaces ([`crate::aero`]), its hydrostatic
    /// pressure p₀ + ρ·g·depth under gravity `g` (m/s²).
    pub fn medium(&self, p: DVec3, g: f64) -> Air {
        let w = self.water;
        let pressure = self.surface_pressure + w.density * g * self.depth(p).max(0.0);
        Air {
            t: w.temperature,
            p: pressure,
            density: w.density,
            viscosity: w.viscosity,
            sound: w.sound,
        }
    }
}

/// A shape of a body that floats: the body, its node, the shape (in the node's frame), its
/// drag coefficient; what the water did in the last step.
#[derive(Clone, Debug)]
pub struct Floater {
    pub body: usize,
    pub node: NodeId,
    pub shape: Shape,
    pub drag: f64,
    /// The volume under water (m³), the force (N) and torque (N·m) it put on the node.
    pub volume: f64,
    pub force: DVec3,
    pub torque: DVec3,
    /// The work the water did on it, summed (J).
    pub work: f64,
    added: f64,
    /// The centre of buoyancy (node frame) and where it was at the step's start.
    centre: DVec3,
    start: DVec3,
    origin: DVec3,
    end: DVec3,
    dashpot: Option<Dashpot>,
}

impl Floater {
    /// `shape` on node `node` of body `body`, with drag coefficient `drag` (1 for a bluff
    /// hull; 0.1–0.3 for a ship's).
    pub fn new(body: usize, node: NodeId, shape: Shape, drag: f64) -> Self {
        Self {
            body,
            node,
            shape,
            drag,
            volume: 0.0,
            force: DVec3::ZERO,
            torque: DVec3::ZERO,
            work: 0.0,
            added: 0.0,
            centre: DVec3::ZERO,
            start: DVec3::ZERO,
            origin: DVec3::ZERO,
            end: DVec3::ZERO,
            dashpot: None,
        }
    }
}

mod couple;
pub use couple::{push, settle};

pub(crate) fn effective_gravity(world: &World, body: usize, x: DVec3) -> DVec3 {
    let s = &world.bodies[body].structure;
    s.gravity + s.planet.as_ref().map_or(DVec3::ZERO, |g| g.acceleration(x))
}

pub(crate) fn waterline(sea: &Sea, rotation: DMat3, centre: DVec3) -> Waterline {
    let (at, up) = sea.surface(centre);
    let normal = rotation.transpose() * up;
    Waterline {
        normal,
        level: normal.dot(rotation.transpose() * (at - centre)),
    }
}
