//! A body of the world (`Body`): a structure, or a rigid body (one node, no members), with its
//! solver, its shapes and its surface.

use super::shape::Shape;
use super::sleep;
use super::surface::Surface;
use crate::structure::{ImplicitSolver, NodeId, Scheme, Structure, StructureBuilder};
use creeer_math::glam::{DMat3, DVec3};

/// A body of the world: a structure (a rigid body: one node, no members) with shapes on its nodes.
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
    pub(super) crushing: bool,
    /// Stepped as a body in contact is (BDF2), even touching nothing: a character standing on
    /// its legs, and what its legs push in a step. Their loads are equal and opposite, and keep
    /// momentum only under one scheme (Newmark averages a load over the step's two ends,
    /// BDF2 takes the end's: a character walking on a crate on ice gave the pair 2.8 kg·m/s).
    pub held: bool,
    /// Put into the world since its last step (`World::add`; not saved). What its contacts
    /// hold when its first step finds them pressed in never existed: the energy guard lets none
    /// of it become motion (`structure/group.rs`; a fallen man's head laid out 13 cm inside the
    /// crate that knocked him down held 222 MJ, and threw the crate off at 44 m/s, G2.11).
    pub born: bool,
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

    /// A rigid body of the given shape, mass and rotational inertia (kg·m², about its node,
    /// in its own frame) at `position`: a body whose mass is not spread evenly through its
    /// shape (a felled tree: its crown's mass hangs on its stem).
    pub fn rigid_with(shape: Shape, mass: f64, inertia: DMat3, position: DVec3, dt: f64) -> Self {
        let structure = StructureBuilder::rigid_body(position, mass, inertia);
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

    pub(super) fn new(structure: Structure, shapes: Vec<(NodeId, Shape)>, dt: f64) -> Self {
        Self {
            solver: ImplicitSolver::new(&structure, Scheme::Newmark, dt, 1),
            structure,
            shapes,
            friction: 0.5,
            restitution: 0.2,
            surface: Surface::STEEL,
            sleep: sleep::Sleep::default(),
            crushing: false,
            held: false,
            born: false,
        }
    }
}
