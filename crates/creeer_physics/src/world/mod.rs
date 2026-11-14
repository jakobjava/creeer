//! The physics world: bodies (structures and rigid bodies), fixed geometry and contacts (ADR-0005).
//!
//! Every step: find contacts at the start of the step (with a speculative margin, so fast
//! bodies meet the face in front of them before they could pass it), join bodies that touch
//! into groups, and step every group as one implicit minimisation, the groups in parallel.

mod anchor;
mod apart;
mod body;
#[cfg(test)]
mod born_tests;
pub mod contact;
#[cfg(test)]
mod crush_tests;
mod detect;
pub mod frame;
mod friction;
#[cfg(test)]
mod friction_tests;
pub mod grip;
#[cfg(test)]
mod grip_tests;
pub mod ground;
mod ground_hull;
mod ground_rise;
mod ground_sample;
#[cfg(test)]
mod ground_tests;
#[cfg(test)]
mod hessian_probe_tests;
pub mod hull;
mod hull_build;
#[cfg(test)]
mod hull_contact_tests;
#[cfg(test)]
mod hull_ground_tests;
#[cfg(test)]
mod hull_motion_tests;
mod hull_narrow;
mod hull_sat;
#[cfg(test)]
mod hull_tests;
mod impact;
pub mod joint;
mod left;
#[cfg(test)]
mod load_tests;
mod membership;
pub mod narrow;
#[cfg(test)]
mod overlap_tests;
mod pass;
#[cfg(test)]
mod pile_tests;
#[cfg(test)]
mod planet_tests;
mod query;
pub mod ray;
mod sat;
mod save;
pub mod shape;
mod sleep;
mod stepping;
pub mod strut;
#[cfg(test)]
mod strut_tests;
pub mod surface;
#[cfg(test)]
mod tests;
mod turn;
#[cfg(test)]
mod turn_tests;

use crate::gravity::PlanetGravity;
use crate::profile::{self, Slot};
use crate::structure::{NodeId, Structure};
pub use apart::Apart;
pub use body::Body;
use contact::{Contact, Contacts};
use creeer_math::glam::{DQuat, DVec3};
use detect::Found;
use ground::Ground;
use left::Left;
pub use query::{Support, Touch};
#[cfg(test)]
use shape::Shape;
use shape::StaticShape;
use std::sync::Arc;
use surface::Surface;

/// Bodies, fixed geometry, and how contacts behave.
#[derive(Clone, Debug)]
pub struct World {
    /// The moving bodies.
    pub bodies: Vec<Body>,
    /// Fixed geometry (ground, walls).
    pub statics: Vec<StaticShape>,
    /// Ground given by a density field (planet terrain; `ground.rs`).
    pub ground: Option<Arc<dyn Ground>>,
    /// Planet gravity for every body added from now on (replacing their uniform gravity).
    pub planet: Option<PlanetGravity>,
    /// Friction coefficient of the fixed geometry.
    pub static_friction: f64,
    /// Restitution of the fixed geometry.
    pub static_restitution: f64,
    /// What the fixed geometry and the ground are made of.
    pub static_surface: Surface,
    /// Contacts of the last step.
    pub contacts: Vec<Contact>,
    /// The groups of the last step: their bodies and contacts (members index the bodies).
    groups: Vec<(Vec<usize>, Contacts)>,
    /// Joints between bodies, or bodies and the world (`joint.rs`).
    joints: Vec<joint::Joint>,
    /// Pairs of bodies that pass through each other beyond a joint's (a ragdoll's limbs). Not
    /// saved: whoever loads a world gives them again, as its ground.
    passing: Vec<(usize, usize)>,
    /// Struts and grips for the next step (`strut.rs`, `grip.rs`).
    struts: Vec<strut::Strut>,
    grips: Vec<grip::Grip>,
    /// Friction anchors of the last step.
    anchors: Vec<anchor::Anchor>,
    /// The elastic energy each point contact and face held at the end of the last step, by
    /// the bodies (and nodes) it joined, and the bodies stepped then (the energy guard's,
    /// `structure/group.rs`).
    left: Left,
    /// The bodies a (sub)step is restricted to while groups turning fast are substepped apart
    /// (`turn.rs`); none: every body. Not saved.
    only: Option<Vec<bool>>,
    dt: f64,
}

impl World {
    /// An empty world stepped by `dt` seconds.
    pub fn new(dt: f64) -> Self {
        Self {
            bodies: Vec::new(),
            statics: Vec::new(),
            ground: None,
            planet: None,
            static_friction: 0.6,
            static_restitution: 0.5,
            static_surface: Surface::CONCRETE,
            contacts: Vec::new(),
            groups: Vec::new(),
            left: Left::default(),
            joints: Vec::new(),
            passing: Vec::new(),
            struts: Vec::new(),
            grips: Vec::new(),
            anchors: Vec::new(),
            only: None,
            dt,
        }
    }

    /// Time step (s).
    pub fn dt(&self) -> f64 {
        self.dt
    }

    /// Adds a body and returns its index.
    pub fn add(&mut self, mut body: Body) -> usize {
        if let Some(planet) = &self.planet {
            body.structure.gravity = DVec3::ZERO;
            body.structure.planet = Some(planet.clone());
            // A planet's potential is measured from each node's rest place. A body built
            // elsewhere and put here (a frame built about the origin: the planet's centre)
            // would have it measured from there: infinite at the centre, and lost in rounding
            // far from it, so the energy guard and the impact budget went blind (a crushed
            // frame's velocities became NaN, F5). Its rest shape is moved, whole, to it.
            let s = &mut body.structure;
            #[allow(clippy::cast_precision_loss)] // a count
            let n = s.position.len() as f64;
            let shift = (s.position.iter().sum::<DVec3>() - s.rest.iter().sum::<DVec3>()) / n;
            for r in &mut s.rest {
                *r += shift;
            }
        }
        // The body may have been changed since its solver was made (velocity, gravity).
        body.solver.reset_history(&body.structure);
        body.born = true;
        self.bodies.push(body);
        self.bodies.len() - 1
    }

    /// Adds a copy of a body as it is in another world (a client's prediction of the server's
    /// bodies), as [`Self::add`] does, but not as new (`Body::born`): what its contacts hold
    /// exists there.
    pub fn add_copy(&mut self, body: Body) -> usize {
        let index = self.add(body);
        self.bodies[index].born = false;
        index
    }

    /// One (sub)step of `self.dt`.
    fn step_once(&mut self) {
        let found = self.detect_awake();
        self.step_found(&found);
    }

    /// The contacts at the start of a (sub)step of `self.dt`, every body they reach woken.
    fn detect_awake(&mut self) -> Vec<Found> {
        let timer = profile::start();
        let _scope = creeer_core::profile::scope("physics: detect");
        self.wake_disturbed();
        // Contacts between an awake and a sleeping body wake the sleeper, which may touch
        // further sleepers: detect until nothing new wakes (a hit at the top of a sleeping
        // stack wakes the whole stack).
        let found = loop {
            // Bodies joined to awake ones wake with them (one group).
            let mut woke = self.wake_joined();
            woke |= self.wake_strutted();
            let found = self.detect();
            for (a, _, b, _) in &found {
                if let Some((b, _)) = b {
                    let (a, b) = (*a, *b);
                    if self.bodies[a].sleep.asleep != self.bodies[b].sleep.asleep {
                        self.bodies[a].wake();
                        self.bodies[b].wake();
                        woke = true;
                    }
                }
            }
            if !woke {
                break found;
            }
        };
        timer.stop(Slot::Detect);
        found
    }

    /// One (sub)step of `self.dt` from the contacts `found` at its start ([`Self::detect_awake`]).
    fn step_found(&mut self, found: &[Found]) {
        profile::add(Slot::WorldSteps, 1);
        let timer = profile::start();
        let scope = creeer_core::profile::scope("physics: groups");
        // A step restricted to some bodies (`only`) keeps the rest's anchors, groups and what
        // their contacts held, as their own step left them.
        let (anchors, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.anchors)
            .into_iter()
            .partition(|a| self.in_step(a.a.0));
        let stepped = self.pass(found, &anchors);
        timer.stop(Slot::Pass);
        drop(scope);
        let timer = profile::start();
        let _scope = creeer_core::profile::scope("physics: after");
        self.anchors = kept;
        match self.only.take() {
            None => {
                self.groups.clear();
                self.left = Left::default();
            }
            Some(only) => {
                let outside = |side: Option<(usize, NodeId)>| side.is_none_or(|(b, _)| !only[b]);
                self.groups.retain(|(group, _)| !only[group[0]]);
                (self.left.energies).retain(|((a, b), _)| outside(*a) && outside(*b));
                self.left.stepped.retain(|&b| !only[b]);
                self.only = Some(only);
            }
        }
        let mut groups = Vec::with_capacity(stepped.len());
        for (group, contacts, anchors) in stepped {
            self.leave(&group, &contacts);
            self.anchors.extend(anchors);
            groups.push(group.clone());
            self.groups.push((group, contacts));
        }
        self.contacts.clear();
        for (_, contacts) in &self.groups {
            self.contacts.extend(contacts.list.iter().copied());
        }
        self.update_sleep(&groups);
        timer.stop(Slot::After);
    }

    /// Whether body `b` is stepped in this (sub)step (`only`).
    fn in_step(&self, b: usize) -> bool {
        self.only.as_ref().is_none_or(|only| only[b])
    }

    /// The groups of the last step: bodies (indices) joined by contacts, stepped together.
    pub fn groups(&self) -> impl Iterator<Item = &[usize]> {
        self.groups.iter().map(|(g, _)| g.as_slice())
    }

    /// Total mechanical energy (J): all bodies, plus the elastic energy stored in contacts.
    pub fn energy(&self) -> f64 {
        use crate::structure::group::{Coupling, MemberState};
        let contacts: f64 = self
            .groups
            .iter()
            .map(|(bodies, contacts)| {
                let states: Vec<MemberState<'_>> = bodies
                    .iter()
                    .map(|&b| MemberState {
                        position: &self.bodies[b].structure.position,
                        orientation: &self.bodies[b].structure.orientation,
                    })
                    .collect();
                contacts.elastic_energy(&states)
            })
            .sum();
        self.bodies
            .iter()
            .map(|b| b.structure.energy())
            .sum::<f64>()
            + contacts
    }

    /// Total momentum of all bodies (kg·m/s).
    pub fn momentum(&self) -> DVec3 {
        self.bodies
            .iter()
            .flat_map(|b| {
                let s = &b.structure;
                (0..s.node_count()).map(move |i| s.velocity[i] * s.mass[i])
            })
            .sum()
    }
}

/// Orientation helper for tests and scenes.
pub fn rotation(axis: DVec3, angle: f64) -> DQuat {
    DQuat::from_axis_angle(axis.normalize(), angle)
}

/// Velocity of the material point of `s` at `point`, carried by `node`.
fn point_velocity(s: &Structure, node: NodeId, point: DVec3) -> DVec3 {
    s.velocity[node] + s.angular_velocity[node].cross(point - s.position[node])
}
