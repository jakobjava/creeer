//! The physics bubble: the one region where full physics runs (plan.md §1, principle 6),
//! in one of two frames (ADR-0007):
//!
//! - **Surface**: turning with a body (centre at the origin, its rotation axis +Y). The
//!   ground stands still, terrain is static collision geometry; the body's gravity, the
//!   frame's centrifugal and Coriolis accelerations, its push and the tides of every other
//!   body act.
//! - **Orbit**: free-falling with a point near the ships (a floating origin on rails, axes
//!   inertial). Gravity is purely tidal there: what differs between a ship and that point.
//!   Coordinates stay small however far from any body the ships are.
//!
//! In the surface frame the bubble also pushes back on its body: what its contents gain in
//! momentum, beyond what the known forces give them, came from the ground and the body's
//! gravity, and the opposite goes into the body (Newton's third law). A ship standing on an
//! asteroid with its engine running moves the asteroid.

use super::LOOKAHEAD;
use super::bodies::Bodies;
use super::pose::Pose;
use super::terrains::BodyRef;
use creeer_math::glam::DVec3;
use creeer_orbit::G;
use creeer_orbit::rails::{Group, Particle};
use creeer_physics::gravity::{PlanetGravity, Tidal};
use creeer_physics::world::World;
use creeer_physics::world::frame::FrameChange;
use creeer_physics::world::ground::Ground;
use std::sync::Arc;

/// The frame the bubble runs in.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // one per universe: boxing would buy nothing
pub enum Frame {
    /// Turning with a body.
    Surface(BodyRef),
    /// Free-falling with a point on rails (inertial axes).
    Orbit(Group),
}

/// Impulses below this change of the body's velocity are collected rather than applied every
/// tick (m/s): a crate resting on a planet must not rewrite its forecast 60 times a second.
const PUSH_THRESHOLD: f64 = 1e-9;
/// Collected impulses are applied at least this often (s).
const PUSH_PERIOD: f64 = 86_400.0;

/// The physics bubble.
#[derive(Debug)]
pub struct Bubble {
    /// The bodies (ships, crates) with full physics.
    pub world: World,
    /// Its frame.
    pub frame: Frame,
    /// Vessel id of each body of the world (same order).
    pub vessels: Vec<u64>,
    /// Counts frame changes (the client moves its camera along).
    pub generation: u64,
    /// The last frame change.
    pub last_change: Option<FrameChange>,
    /// Impulse collected for the body of a surface frame (inertial, N·s), since `since`.
    reaction: DVec3,
    since: f64,
}

impl Bubble {
    /// An empty bubble on the surface of `body`, stepped by `dt`.
    pub fn on_surface(body: BodyRef, dt: f64, ground: Option<Arc<dyn Ground>>, t: f64) -> Self {
        let mut world = World::new(dt);
        world.static_friction = 0.7;
        world.ground = ground;
        Self {
            world,
            frame: Frame::Surface(body),
            vessels: Vec::new(),
            generation: 0,
            last_change: None,
            reaction: DVec3::ZERO,
            since: t,
        }
    }

    /// The body a surface frame turns with.
    pub fn anchor(&self) -> Option<BodyRef> {
        match self.frame {
            Frame::Surface(body) => Some(body),
            Frame::Orbit(_) => None,
        }
    }

    /// The frame in inertial space at time `t`.
    pub fn pose(&self, bodies: &Bodies, t: f64) -> Pose {
        match &self.frame {
            Frame::Surface(body) => bodies.pose(*body, t),
            Frame::Orbit(origin) => {
                // The origin is on rails at the start of the step; the field is wanted mid-step.
                let (x, v) = bodies.extrapolate(origin, t);
                Pose::inertial(x, v)
            }
        }
    }

    /// Gravity in the frame at time `t`: the anchor's own (surface frames), the tides of every
    /// major body, and the anchor's push.
    pub fn field(&self, bodies: &Bodies, t: f64) -> PlanetGravity {
        let pose = self.pose(bodies, t);
        let back = pose.rotation.conjugate();
        let anchor = self.anchor();
        let mut tidal = Vec::with_capacity(bodies.ephemeris.len());
        for (i, body) in bodies.ephemeris.bodies().iter().enumerate() {
            if anchor == Some(BodyRef::Major(i)) {
                continue;
            }
            let at = back * (bodies.ephemeris.position(i, t) - pose.position);
            tidal.push(Tidal { mu: body.mu, at });
        }
        let (mu, spin, uniform) = match anchor {
            Some(body) => (
                bodies.body(body).mu,
                back * pose.spin,
                -(back * bodies.push_acceleration(body, t)),
            ),
            None => (0.0, DVec3::ZERO, DVec3::ZERO),
        };
        PlanetGravity {
            mu,
            spin,
            uniform,
            tidal: tidal.into(),
        }
    }

    /// One physics step from `t` to `t + h`: the field, the world, the moving origin, and the
    /// push back on the anchor.
    ///
    /// The field is the one at the end of the step: Newmark pairs the acceleration at the
    /// start (found in the previous step) with the force at the end, which the step solves
    /// for. A field frozen at mid-step made every force half a step early: a ball 800 m from
    /// a free-falling origin drifted 7 mm from its rails in 2 minutes, halving with the step
    /// (first order); with the field at the end it is second order (ADR-0007).
    pub fn step(&mut self, bodies: &mut Bodies, t: f64) {
        let h = self.world.dt();
        let start_field = self.field(bodies, t);
        let field = self.field(bodies, t + h);
        self.world.set_planet(Some(field.clone()));
        let before = self.balance(&start_field);
        self.world.step();
        let after = self.balance(&field);
        let end = t + h;
        match &mut self.frame {
            Frame::Orbit(origin) => {
                origin.propagate(&bodies.ephemeris, end, None);
            }
            Frame::Surface(body) => {
                if let BodyRef::Minor(i) = *body {
                    bodies.minors[i]
                        .group
                        .propagate(&bodies.ephemeris, end, None);
                }
                // dP = J(anchor's gravity + ground) + J(everything else); the anchor gets
                // −J(anchor's gravity + ground) = −dP + J(everything else).
                let impulse =
                    -(after.momentum - before.momentum) + (before.other + after.other) * (0.5 * h);
                let rotation = bodies.orientation(*body, t + 0.5 * h);
                self.reaction += rotation * impulse;
            }
        }
    }

    /// Momentum of the world's contents and the force on them from everything but the
    /// anchor's gravity and the ground (frame axes).
    fn balance(&self, field: &PlanetGravity) -> Balance {
        let mut momentum = DVec3::ZERO;
        let mut other = DVec3::ZERO;
        for body in &self.world.bodies {
            let s = &body.structure;
            for (i, &m) in s.masses().iter().enumerate() {
                let (x, v) = (s.position[i], s.velocity[i]);
                momentum += v * m;
                let central = if field.mu > 0.0 {
                    let r = x.length();
                    -x * (field.mu / (r * r * r))
                } else {
                    DVec3::ZERO
                };
                other += (field.acceleration(x) - central + field.coriolis(v)) * m + s.load[i];
            }
        }
        Balance { momentum, other }
    }

    /// The impulse collected for the anchor and not applied yet (inertial, N·s).
    pub fn pending_reaction(&self) -> DVec3 {
        self.reaction
    }

    /// Applies the collected push on the anchor if it is large enough, old enough, or
    /// `force`d (before leaving the frame), as a force over the next tick: delayed by at most
    /// one period, never lost.
    pub fn flush_reaction(&mut self, bodies: &mut Bodies, now: f64, tick: f64, force: bool) {
        let Some(anchor) = self.anchor() else {
            self.reaction = DVec3::ZERO;
            self.since = now;
            return;
        };
        if self.reaction == DVec3::ZERO {
            self.since = now;
            return;
        }
        let mass = bodies.body(anchor).mu / G;
        let due = force
            || self.reaction.length() / mass > PUSH_THRESHOLD
            || now - self.since > PUSH_PERIOD;
        if due {
            bodies.push(anchor, now, now + tick, self.reaction / tick, LOOKAHEAD);
            self.reaction = DVec3::ZERO;
            self.since = now;
        }
    }

    /// Moves the bubble into another frame at time `t` (`ground` for the new one).
    pub fn change_frame(
        &mut self,
        bodies: &Bodies,
        t: f64,
        frame: Frame,
        ground: Option<Arc<dyn Ground>>,
    ) {
        let old = self.pose(bodies, t);
        let new_pose = match &frame {
            Frame::Surface(body) => bodies.pose(*body, t),
            Frame::Orbit(origin) => {
                let (x, v) = origin.state(0);
                Pose::inertial(x, v)
            }
        };
        let change = old.change_to(&new_pose);
        self.world.change_frame(&change);
        self.world.ground = ground;
        self.frame = frame;
        // The new frame's field at once: the solvers restart from accelerations in it.
        let field = self.field(bodies, t);
        self.world.set_planet(Some(field));
        for body in &mut self.world.bodies {
            body.solver.reset_history(&body.structure);
        }
        self.generation += 1;
        self.last_change = Some(change);
    }

    /// A free-falling origin at an inertial state at time `t`.
    pub fn origin(t: f64, x: DVec3, v: DVec3) -> Group {
        Group::new(t, &[(x, v)], vec![Particle { mass: 1.0, mu: 0.0 }])
    }
}

struct Balance {
    momentum: DVec3,
    other: DVec3,
}
