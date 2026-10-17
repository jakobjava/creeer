//! Building structures node by node and member by member.

use super::element::Element;
use super::{Material, NodeId, STANDARD_GRAVITY, Section, Structure, Support};
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// Builds a [`Structure`] node by node and member by member.
#[derive(Clone, Debug, Default)]
pub struct StructureBuilder {
    positions: Vec<DVec3>,
    supports: Vec<Support>,
    elements: Vec<Element>,
    /// Extra mass and rotational inertia (rest frame) per node: equipment, cargo, or the
    /// whole body of a rigid body.
    lumps: Vec<(f64, DMat3)>,
}

impl StructureBuilder {
    /// An empty structure.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a free node at `position`.
    pub fn node(&mut self, position: DVec3) -> NodeId {
        self.positions.push(position);
        self.supports.push(Support::Free);
        self.lumps.push((0.0, DMat3::ZERO));
        self.positions.len() - 1
    }

    /// Adds a lumped mass (kg) with rotational inertia `inertia` (kg·m², about the node, in
    /// the rest frame) to `node`.
    pub fn mass(&mut self, node: NodeId, mass: f64, inertia: DMat3) -> &mut Self {
        self.lumps[node].0 += mass;
        self.lumps[node].1 += inertia;
        self
    }

    /// A rigid body: one node at its centre of mass, no members.
    pub fn rigid_body(position: DVec3, mass: f64, inertia: DMat3) -> Structure {
        let mut b = Self::new();
        let node = b.node(position);
        b.mass(node, mass, inertia);
        b.build()
    }

    /// Holds `node` in place.
    pub fn support(&mut self, node: NodeId, support: Support) -> &mut Self {
        self.supports[node] = support;
        self
    }

    /// Joins `a` and `b` rigidly with a member. The section's y axis points as close to
    /// world up (+y) as possible, or along +x for vertical members.
    pub fn beam(
        &mut self,
        a: NodeId,
        b: NodeId,
        section: Section,
        material: Material,
    ) -> &mut Self {
        let along = (self.positions[b] - self.positions[a]).normalize();
        let up = if along.y.abs() > 0.99 {
            DVec3::X
        } else {
            DVec3::Y
        };
        self.beam_oriented(a, b, section, material, up)
    }

    /// Like [`Self::beam`], with the section's y axis pointing as close to `up` as possible.
    pub fn beam_oriented(
        &mut self,
        a: NodeId,
        b: NodeId,
        section: Section,
        material: Material,
        up: DVec3,
    ) -> &mut Self {
        let element = Element::new(
            (a, self.positions[a]),
            (b, self.positions[b]),
            up,
            section,
            material,
        );
        self.elements.push(element);
        self
    }

    /// Finishes the structure, lumping member masses onto the nodes.
    ///
    /// # Panics
    /// If a node has neither a member nor a lumped mass.
    pub fn build(self) -> Structure {
        let n = self.positions.len();
        let mut mass = vec![0.0; n];
        let mut inertia = vec![DMat3::ZERO; n];
        for e in &self.elements {
            let node_inertia = e.node_inertia();
            for node in [e.a, e.b] {
                mass[node] += 0.5 * e.mass();
                inertia[node] += node_inertia;
            }
        }
        for (node, (m, j)) in self.lumps.iter().enumerate() {
            mass[node] += m;
            inertia[node] += *j;
        }
        assert!(
            mass.iter().all(|m| *m > 0.0),
            "every node needs a member or a lumped mass"
        );
        let (free_translation, free_rotation) = self.supports.iter().map(|s| s.masks()).unzip();
        Structure {
            orientation: vec![DQuat::IDENTITY; n],
            velocity: vec![DVec3::ZERO; n],
            angular_velocity: vec![DVec3::ZERO; n],
            load: vec![DVec3::ZERO; n],
            torque: vec![DVec3::ZERO; n],
            drag: vec![0.0; n],
            dashpots: Vec::new(),
            gravity: STANDARD_GRAVITY,
            planet: None,
            rest: self.positions.clone(),
            position: self.positions,
            mass,
            inertia,
            free_translation,
            free_rotation,
            elements: self.elements,
            splits: true,
        }
    }
}
