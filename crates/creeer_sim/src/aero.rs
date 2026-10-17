//! Air on bodies (F6.6, ADR-0014): surfaces ([`creeer_fluids::aero`]) fixed to nodes, the air
//! from the planet's atmosphere where each one is, at rest in the planet's frame (it turns
//! with the planet) but for the wind.
//!
//! Before the world's step each surface's force, at the motion its node starts with, goes
//! on the node as a load (and, off the node, a torque); its damping slope goes with it as a
//! dashpot, so the step takes the drag implicitly: a light panel in a fast stream is as
//! stable as a heavy one. After the step they come off again, and the work the air did is
//! counted.

use creeer_fluids::aero::Surface;
use creeer_fluids::atmosphere::{Air, Atmosphere};
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::{Dashpot, NodeId, Scheme};
use creeer_physics::world::World;

/// Where altitude is measured from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Datum {
    /// A flat world: the altitude is y.
    Flat,
    /// A planet centred at the origin: the distance from it less its radius (m).
    Sphere { radius: f64 },
}

/// The air around the world's bodies.
#[derive(Clone, Debug)]
pub struct Sky {
    pub atmosphere: Atmosphere,
    pub datum: Datum,
    /// The wind (m/s, the world's frame), the same everywhere.
    pub wind: DVec3,
}

impl Sky {
    /// The air at `position`.
    pub fn air(&self, position: DVec3) -> Air {
        let z = match self.datum {
            Datum::Flat => position.y,
            Datum::Sphere { radius } => position.length() - radius,
        };
        self.atmosphere.at(z)
    }
}

/// A surface fixed to node `node` of body `body`, and what it did in the last step.
#[derive(Clone, Debug)]
pub struct Airframe {
    pub body: usize,
    pub node: NodeId,
    pub surface: Surface,
    /// The force (N, world) and torque (N·m) it put on the node at the step's start.
    pub force: DVec3,
    pub torque: DVec3,
    /// Where its point was at the step's start, the dashpot it gave, the force at the last
    /// step's end.
    point: DVec3,
    end: DVec3,
    dashpot: Option<Dashpot>,
    /// The work the air did on it, summed (J; negative: drag took it).
    pub work: f64,
}

impl Airframe {
    /// `surface` on node `node` of body `body`.
    pub fn new(body: usize, node: NodeId, surface: Surface) -> Self {
        Self {
            body,
            node,
            surface,
            force: DVec3::ZERO,
            torque: DVec3::ZERO,
            point: DVec3::ZERO,
            end: DVec3::ZERO,
            dashpot: None,
            work: 0.0,
        }
    }

    fn point(&self, world: &World) -> (DMat3, DVec3, DVec3) {
        let s = &world.bodies[self.body].structure;
        let r = DMat3::from_quat(s.orientation[self.node]);
        let arm = r * self.surface.offset;
        (r, arm, s.position[self.node] + arm)
    }
}

fn cross(r: DVec3) -> DMat3 {
    DMat3::from_cols(
        DVec3::new(0.0, r.z, -r.y),
        DVec3::new(-r.z, 0.0, r.x),
        DVec3::new(r.y, -r.x, 0.0),
    )
}

/// Before the world's step: every surface's force and torque on its node, its damping as a
/// dashpot (on the node's (v, ω): the surface's point moves at v + ω × r).
pub fn push(sky: &Sky, world: &mut World, frames: &mut [Airframe]) {
    for f in frames.iter_mut() {
        let (rotation, arm, point) = f.point(world);
        let s = &mut world.bodies[f.body].structure;
        let (v, w) = (s.velocity[f.node], s.angular_velocity[f.node]);
        let moving = v + w.cross(arm) - sky.wind;
        let (force, d) = f.surface.linearised(rotation, moving, &sky.air(point));
        f.force = force;
        f.torque = arm.cross(force);
        f.point = point;
        s.load[f.node] += f.force;
        s.torque[f.node] += f.torque;
        // (v, ω) ↦ v + ω × r = v − [r]×·ω: the dashpot Jᵀ·D·J with J = [I, −[r]×].
        let r = cross(arm);
        let blocks = [[d, -(d * r)], [r * d, -(r * d * r)]];
        let mut matrix = [[0.0; 6]; 6];
        for (i, row) in matrix.iter_mut().enumerate() {
            for (j, m) in row.iter_mut().enumerate() {
                *m = blocks[i / 3][j / 3].col(j % 3)[i % 3];
            }
        }
        let dashpot = Dashpot {
            node: f.node,
            matrix,
            velocity: v,
            angular: w,
        };
        s.dashpots.push(dashpot);
        f.dashpot = Some(dashpot);
    }
}

/// After the world's step: the forces, torques and dashpots off again, and the work the air
/// did over the step booked along the way its point went: by the force the step took. Its
/// end force is the start's less the dashpot's share; Newmark took the mean of the last
/// step's end and this one's start, and the dashpot's whole share (`dashpot.rs` in the
/// physics), which is the books' force exactly; backward Euler, the end's.
pub fn settle(world: &mut World, frames: &mut [Airframe]) {
    for f in frames.iter_mut().rev() {
        let (_, _, point) = f.point(world);
        let s = &mut world.bodies[f.body].structure;
        s.load[f.node] -= f.force;
        s.torque[f.node] -= f.torque;
        let Some(d) = f.dashpot.take() else { continue };
        if let Some(k) = s.dashpots.iter().rposition(|x| *x == d) {
            s.dashpots.remove(k);
        }
        let (dv, dw) = (
            s.velocity[f.node] - d.velocity,
            s.angular_velocity[f.node] - d.angular,
        );
        let delta = [dv.x, dv.y, dv.z, dw.x, dw.y, dw.z];
        let pushed: [f64; 3] =
            std::array::from_fn(|i| (0..6).map(|j| d.matrix[i][j] * delta[j]).sum());
        let pushed = DVec3::from_array(pushed);
        let end = f.force - pushed;
        let taken = match world.bodies[f.body].solver.last_scheme() {
            Scheme::Newmark => 0.5 * (f.end + f.force) - pushed,
            _ => end,
        };
        f.work += taken.dot(point - f.point);
        f.end = end;
    }
}
