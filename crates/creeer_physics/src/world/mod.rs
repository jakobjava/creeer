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
mod impact;
pub mod narrow;
mod pass;
#[cfg(test)]
mod pile_tests;
mod sat;
pub mod shape;
mod sleep;
mod stepping;
#[cfg(test)]
mod tests;

use crate::structure::{ImplicitSolver, NodeId, Scheme, Structure, StructureBuilder};
use contact::{Contact, Contacts};
use creeer_math::glam::{DQuat, DVec3};
use shape::{Shape, StaticShape};
/// Largest turn of a box within one (sub)step (rad).
const MAX_TURN: f64 = 0.05;
/// Most substeps per step.
const MAX_SUBSTEPS: usize = 16;

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

    /// A structure whose joints collide as balls as thick as their thickest member.
    pub fn structure(structure: Structure, dt: f64) -> Self {
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
        Self::new(structure, shapes, dt)
    }

    fn new(structure: Structure, shapes: Vec<(NodeId, Shape)>, dt: f64) -> Self {
        Self {
            solver: ImplicitSolver::new(&structure, Scheme::Newmark, dt, 1),
            structure,
            shapes,
            friction: 0.5,
            restitution: 0.2,
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
    /// Friction coefficient of the fixed geometry.
    pub static_friction: f64,
    /// Restitution of the fixed geometry.
    pub static_restitution: f64,
    /// Penalty stiffness per contact point (N/m): 1e8 gives 1 mm of penetration per 100 kN.
    pub contact_stiffness: f64,
    /// Contacts of the last step.
    pub contacts: Vec<Contact>,
    /// The groups of the last step: their bodies and contacts (member indices refer to the
    /// bodies listed).
    groups: Vec<(Vec<usize>, Contacts)>,
    /// Friction anchors of the last step.
    anchors: Vec<anchor::Anchor>,
    dt: f64,
}

impl World {
    /// An empty world stepped by `dt` seconds.
    pub fn new(dt: f64) -> Self {
        Self {
            bodies: Vec::new(),
            statics: Vec::new(),
            static_friction: 0.6,
            static_restitution: 0.5,
            contact_stiffness: 1.0e8,
            contacts: Vec::new(),
            groups: Vec::new(),
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
        // The body may have been changed since its solver was made (velocity, gravity).
        body.solver.reset_history(&body.structure);
        self.bodies.push(body);
        self.bodies.len() - 1
    }

    /// Advances the world by one step, split into substeps while boxes turn fast: contacts
    /// are found at the start of a (sub)step with fixed normals and points, which describe a
    /// box that turns 0.3 rad in the step (a crate struck by a ball, 18 rad/s) badly enough
    /// to let corners pass into neighbours unnoticed. Substeps keep the turn below
    /// [`MAX_TURN`].
    pub fn step(&mut self) {
        let turn = self
            .bodies
            .iter()
            .filter(|b| !b.sleep.asleep)
            .flat_map(|b| {
                let s = &b.structure;
                b.shapes
                    .iter()
                    .filter(|(_, shape)| matches!(shape, Shape::Box { .. }))
                    .map(move |&(node, _)| s.angular_velocity[node].length())
            })
            .fold(0.0, f64::max)
            * self.dt;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, clamped
        let n = ((turn / MAX_TURN).ceil() as usize).clamp(1, MAX_SUBSTEPS);
        if n == 1 {
            self.step_once();
            return;
        }
        let dt = self.dt;
        #[allow(clippy::cast_precision_loss)]
        let h = dt / n as f64;
        for b in &mut self.bodies {
            b.solver.set_time_step(&b.structure, h);
        }
        self.dt = h;
        for _ in 0..n {
            self.step_once();
        }
        self.dt = dt;
        for b in &mut self.bodies {
            b.solver.set_time_step(&b.structure, dt);
        }
    }

    /// One (sub)step of `self.dt`.
    fn step_once(&mut self) {
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
        let anchors = std::mem::take(&mut self.anchors);
        let stepped = self.pass(&found, &anchors);
        self.contacts.clear();
        self.groups.clear();
        let mut groups = Vec::with_capacity(stepped.len());
        for (group, contacts, anchors) in stepped {
            self.anchors.extend(anchors);
            self.contacts.extend(contacts.list.iter().copied());
            groups.push(group.clone());
            self.groups.push((group, contacts));
        }
        self.update_sleep(&groups);
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
