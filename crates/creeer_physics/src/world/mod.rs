//! The physics world: bodies (structures and rigid bodies), fixed geometry and contacts
//! (ADR-0005).
//!
//! Every step: find contacts at the start of the step (with a speculative margin, so fast
//! bodies meet the face in front of them before they could pass it), join bodies that touch
//! into groups, and step every group as one implicit minimisation, the groups in parallel.

mod anchor;
pub mod contact;
#[cfg(test)]
mod crush_tests;
mod detect;
pub mod frame;
mod friction;
#[cfg(test)]
mod friction_tests;
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
mod sat;
mod save;
pub mod shape;
mod sleep;
mod stepping;
pub mod surface;
#[cfg(test)]
mod tests;
mod turn;

use crate::gravity::PlanetGravity;
use crate::profile::{self, Slot};
use crate::structure::{ImplicitSolver, NodeId, Scheme, Structure, StructureBuilder};
use contact::{Contact, Contacts};
use creeer_math::glam::{DQuat, DVec3};
use ground::Ground;
use left::Left;
pub use query::{Support, Touch};
use shape::{Shape, StaticShape};
use std::sync::Arc;
use surface::Surface;

/// A body of the world: a structure (or a rigid body: one node, no members) with collision
/// shapes on its nodes.
#[derive(Clone, Debug)]
pub struct Body {
    /// The physical body.
    pub structure: Structure,
    /// Its time stepper.
    pub solver: ImplicitSolver,
    /// Collision shapes and the nodes they belong to.
    pub shapes: Vec<(NodeId, Shape)>,
    /// Coulomb friction coefficient of its surface.
    pub friction: f64,
    /// Coefficient of restitution of its surface (0: no bounce, 1: elastic).
    pub restitution: f64,
    /// What its surface is made of (contact stiffness, `surface.rs`).
    pub surface: Surface,
    /// Whether it is asleep (at rest, not stepped).
    pub sleep: sleep::Sleep,
    /// Whether its last step was an impact in which members still flowed plastically.
    crushing: bool,
}

impl Body {
    /// Whether its last step was an impact in which members still flowed plastically (it is
    /// then stepped in impact substeps).
    pub fn crushing(&self) -> bool {
        self.crushing
    }

    /// A rigid body of the given shape and mass at `position`.
    pub fn rigid(shape: Shape, mass: f64, position: DVec3, dt: f64) -> Self {
        let structure = StructureBuilder::rigid_body(position, mass, shape.inertia(mass));
        Self::new(structure, vec![(0, shape)], dt)
    }

    /// A structure whose joints collide as balls as thick as their thickest member, with the
    /// surface of its members' material (the first member's).
    pub fn structure(structure: Structure, dt: f64) -> Self {
        let surface = structure
            .elements
            .first()
            .map_or(Surface::STEEL, |e| Surface::of(&e.material));
        let mut radius = vec![0.0f64; structure.node_count()];
        for e in &structure.elements {
            let r = e.section.extent[0].max(e.section.extent[1]);
            radius[e.a] = radius[e.a].max(r);
            radius[e.b] = radius[e.b].max(r);
        }
        let shapes = radius
            .into_iter()
            .enumerate()
            .filter(|(_, r)| *r > 0.0)
            .map(|(node, radius)| (node, Shape::Sphere { radius }))
            .collect();
        let mut body = Self::new(structure, shapes, dt);
        body.surface = surface;
        body
    }

    fn new(structure: Structure, shapes: Vec<(NodeId, Shape)>, dt: f64) -> Self {
        Self {
            solver: ImplicitSolver::new(&structure, Scheme::Newmark, dt, 1),
            structure,
            shapes,
            friction: 0.5,
            restitution: 0.2,
            surface: Surface::STEEL,
            sleep: sleep::Sleep::default(),
            crushing: false,
        }
    }
}

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
    /// The groups of the last step: their bodies and contacts (member indices refer to the
    /// bodies listed).
    groups: Vec<(Vec<usize>, Contacts)>,
    /// Friction anchors of the last step.
    anchors: Vec<anchor::Anchor>,
    /// The elastic energy each point contact and face held at the end of the last step, by
    /// the bodies (and nodes) it joined, and the bodies stepped then (the energy guard's,
    /// `structure/group.rs`).
    left: Left,
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
            anchors: Vec::new(),
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
        self.bodies.push(body);
        self.bodies.len() - 1
    }

    /// One (sub)step of `self.dt`.
    fn step_once(&mut self) {
        profile::add(Slot::WorldSteps, 1);
        let timer = profile::start();
        self.wake_disturbed();
        // Contacts between an awake and a sleeping body wake the sleeper, which may touch
        // further sleepers: detect until nothing new wakes (a hit at the top of a sleeping
        // stack wakes the whole stack).
        let found = loop {
            let found = self.detect();
            let mut woke = false;
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
        let timer = profile::start();
        let anchors = std::mem::take(&mut self.anchors);
        let stepped = self.pass(&found, &anchors);
        timer.stop(Slot::Pass);
        let timer = profile::start();
        self.contacts.clear();
        self.groups.clear();
        self.left = Left::default();
        let mut groups = Vec::with_capacity(stepped.len());
        for (group, contacts, anchors) in stepped {
            self.leave(&group, &contacts);
            self.anchors.extend(anchors);
            self.contacts.extend(contacts.list.iter().copied());
            groups.push(group.clone());
            self.groups.push((group, contacts));
        }
        self.update_sleep(&groups);
        timer.stop(Slot::After);
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
