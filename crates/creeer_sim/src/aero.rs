//! Air on bodies (F6.6, ADR-0014): surfaces ([`creeer_fluids::aero`]) fixed to nodes, the air
//! from the planet's atmosphere where each one is, at rest in the planet's frame (it turns
//! with the planet) but for the wind.
//!
//! Before the world's step each surface's force, at the motion its node starts with, goes
//! on the node as a load (and, off the node, a torque); its damping slope goes with it as a
//! dashpot, so the step takes the drag implicitly: a light panel in a fast stream is as
//! stable as a heavy one. After the step they come off again, and the work the air did is
//! counted.

use crate::sea::{Sea, effective_gravity};
use creeer_fluids::aero::Surface;
use creeer_fluids::atmosphere::{Air, Atmosphere};
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::{Dashpot, NodeId, Scheme};
use creeer_physics::world::World;
use rayon::prelude::*;

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
    /// step's end; the node's dashpot it put on with the surfaces after it on the same node
    /// (one for them all, where it went and what it was).
    point: DVec3,
    end: DVec3,
    dashpot: Option<Dashpot>,
    placed: Option<(usize, Dashpot, DVec3, DVec3)>,
    /// Whether its last step's work is still to be booked.
    pending: bool,
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
            placed: None,
            pending: false,
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
/// Books the work the air did on `f` over the step since its push (its point's way, by the
/// force the step took: its end force is the start's less the dashpot's share; Newmark took
/// the mean of the last step's end and this one's start, and the dashpot's whole share
/// (`dashpot.rs` in the physics), which is the books' force exactly; backward Euler, the end's).
fn book_one(world: &World, f: &mut Airframe) {
    if !std::mem::take(&mut f.pending) {
        return;
    }
    let Some(d) = f.dashpot else { return };
    let (_, _, point) = f.point(world);
    let newmark = world.bodies[f.body].solver.last_scheme() == Scheme::Newmark;
    let s = &world.bodies[f.body].structure;
    let (dv, dw) = (
        s.velocity[f.node] - d.velocity,
        s.angular_velocity[f.node] - d.angular,
    );
    let delta = [dv.x, dv.y, dv.z, dw.x, dw.y, dw.z];
    let pushed = DVec3::from_array(std::array::from_fn(|i| {
        (0..6).map(|j| d.matrix[i][j] * delta[j]).sum()
    }));
    let end = f.force - pushed;
    let taken = if newmark {
        0.5 * (f.end + f.force) - pushed
    } else {
        end
    };
    f.work += taken.dot(point - f.point);
    f.end = end;
}

/// Books the work of the last step (which [`push`] does for every surface before its next):
/// for reading [`Airframe::work`] up to date.
pub fn book(world: &World, frames: &mut [Airframe]) {
    for f in frames {
        book_one(world, f);
    }
}

/// One surface's force, torque and dashpot (on the node's (v, ω): its point moves at
/// v + ω × r, so the dashpot is Jᵀ·D·J with J = [I, −[r]×]).
fn surface(sky: &Sky, sea: Option<&Sea>, world: &World, f: &mut Airframe) {
    let (rotation, arm, point) = f.point(world);
    let s = &world.bodies[f.body].structure;
    let (v, w) = (s.velocity[f.node], s.angular_velocity[f.node]);
    // Under the sea, the water (still: currents are a later phase's) is the medium.
    let under = sea.filter(|sea| sea.depth(point) > 0.0);
    let (moving, medium) = match under {
        Some(sea) => {
            let g = effective_gravity(world, f.body, point).length();
            (v + w.cross(arm), sea.medium(point, g))
        }
        None => (v + w.cross(arm) - sky.wind, sky.air(point)),
    };
    let (force, d) = f.surface.linearised(rotation, moving, &medium);
    f.force = force;
    f.torque = arm.cross(force);
    f.point = point;
    let r = cross(arm);
    let (dr, rd) = (d * r, r * d);
    let blocks = [[d, -dr], [rd, -(rd * r)]];
    let mut matrix = [[0.0; 6]; 6];
    for (bi, row) in blocks.iter().enumerate() {
        for (bj, b) in row.iter().enumerate() {
            let cols = b.to_cols_array_2d();
            for (j, col) in cols.iter().enumerate() {
                for (i, v) in col.iter().enumerate() {
                    matrix[3 * bi + i][3 * bj + j] = *v;
                }
            }
        }
    }
    f.dashpot = Some(Dashpot {
        node: f.node,
        matrix,
        velocity: v,
        angular: w,
    });
}

/// Before the world's step: every surface's force and torque on its node, in the air or (below
/// `sea`'s surface) the water, its damping as a dashpot. The surfaces in a row on one node
/// are done together (in parallel with the other nodes': they only read the world) and share
/// one dashpot (the solver's work is per dashpot).
pub fn push(sky: &Sky, sea: Option<&Sea>, world: &mut World, frames: &mut [Airframe]) {
    let seen: &World = world;
    let mut runs: Vec<&mut [Airframe]> = frames
        .chunk_by_mut(|a, b| a.body == b.body && a.node == b.node)
        .collect();
    let sums: Vec<(DVec3, DVec3, Dashpot)> = (runs.par_iter_mut().with_min_len(2))
        .map(|run| {
            let (mut load, mut torque) = (DVec3::ZERO, DVec3::ZERO);
            let mut all: Option<Dashpot> = None;
            for f in run.iter_mut() {
                book_one(seen, f);
                surface(sky, sea, seen, f);
                (load, torque) = (load + f.force, torque + f.torque);
                let d = f.dashpot.expect("found");
                match all.as_mut() {
                    Some(a) => {
                        for (row, add) in a.matrix.iter_mut().zip(&d.matrix) {
                            row.iter_mut().zip(add).for_each(|(x, y)| *x += y);
                        }
                    }
                    None => all = Some(d),
                }
            }
            (load, torque, all.expect("a run has a surface"))
        })
        .collect();
    for (run, (load, torque, dashpot)) in runs.iter_mut().zip(sums) {
        let first = &mut run[0];
        let s = &mut world.bodies[first.body].structure;
        s.load[first.node] += load;
        s.torque[first.node] += torque;
        first.placed = Some((s.dashpots.len(), dashpot, load, torque));
        s.dashpots.push(dashpot);
    }
}

/// After the world's step: the forces, torques and dashpots off again (last on, first off).
/// The step's work is booked at the next [`push`] (or [`book`]), in parallel with the forces.
pub fn settle(world: &mut World, frames: &mut [Airframe]) {
    for f in frames.iter_mut().rev() {
        f.pending = f.dashpot.is_some();
        let Some((slot, placed, load, torque)) = f.placed.take() else {
            continue;
        };
        let s = &mut world.bodies[f.body].structure;
        s.load[f.node] -= load;
        s.torque[f.node] -= torque;
        // Where push put it, unless something else came off out of turn.
        if s.dashpots.get(slot) == Some(&placed) {
            s.dashpots.remove(slot);
        } else if let Some(k) = s.dashpots.iter().rposition(|x| *x == placed) {
            s.dashpots.remove(k);
        }
    }
}
