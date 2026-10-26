//! Growing a structure as it is built (G2.3, ADR-0033): a node where a joint will be, another
//! structure taken in, and a joint between two of its nodes, each as the structure is now (bent,
//! turned, moving), so building onto a loaded structure keeps what it carries.

use super::element::Element;
use super::{JointLaw, NodeId, Structure};
use creeer_math::glam::DQuat;

impl Structure {
    /// A node on member `e` at `t` of its length from its first end (0 < t < 1): the member
    /// becomes two, as it lies now (the cubic through its ends and their tangents), its
    /// plastic deformation shared and its mass and momentum kept. Returns the new node.
    ///
    /// # Panics
    /// If `t` is not within the member, or the member is a joint.
    pub fn insert_node(&mut self, e: usize, t: f64) -> NodeId {
        let el = self.elements[e];
        assert!(
            t > 0.0 && t < 1.0 && !el.is_joint(),
            "a node within a member"
        );
        let (a, b) = (el.a, el.b);
        let (qa, qb) = (self.orientation[a], self.orientation[b]);
        let (ta, tb) = (qa * el.axis, qb * el.axis);
        // Hermite's cubic through the ends with their tangents, at t, and its tangent there.
        let (pa, pb) = (self.position[a], self.position[b]);
        let l = (pb - pa).length();
        let (t2, t3) = (t * t, t * t * t);
        let at = pa * (2.0 * t3 - 3.0 * t2 + 1.0)
            + ta * (l * (t3 - 2.0 * t2 + t))
            + pb * (3.0 * t2 - 2.0 * t3)
            + tb * (l * (t3 - t2));
        let tangent = ((pb - pa) * (6.0 * t - 6.0 * t2)
            + ta * (l * (3.0 * t2 - 4.0 * t + 1.0))
            + tb * (l * (3.0 * t2 - 2.0 * t)))
            .normalize();
        let blend = qa.slerp(qb, t);
        let q = (DQuat::from_rotation_arc(blend * el.axis, tangent) * blend).normalize();
        let m = self.node_count();
        self.position.push(at);
        self.orientation.push(q);
        self.velocity
            .push(self.velocity[a] * (1.0 - t) + self.velocity[b] * t);
        self.angular_velocity
            .push(self.angular_velocity[a] * (1.0 - t) + self.angular_velocity[b] * t);
        self.load.push(creeer_math::glam::DVec3::ZERO);
        self.torque.push(creeer_math::glam::DVec3::ZERO);
        self.drag.push(0.0);
        self.rest.push(self.rest[a] * (1.0 - t) + self.rest[b] * t);
        self.free_translation.push(creeer_math::glam::DVec3::ONE);
        self.free_rotation.push(creeer_math::glam::DVec3::ONE);
        let piece = |(i, pi), (j, pj)| {
            let mut h = Element::new((i, pi), (j, pj), el.y, el.section, el.material);
            h.plastic_strain = el.plastic_strain;
            h.crooked = el.crooked;
            h.thermal = el.thermal * (pj - pi).length() / el.rest_length;
            h
        };
        let mut first = piece((a, self.rest[a]), (m, self.rest[m]));
        let mut second = piece((m, self.rest[m]), (b, self.rest[b]));
        // Plastic deformation: the stretch and twist by length, each end's bend kept at it.
        let (p, r) = (el.plastic, el.flow_rate);
        let shares = |v: [f64; 6], f: f64, end: usize| {
            let mut out = [f * v[0], f * v[1], 0.0, 0.0, 0.0, 0.0];
            out[2 + end] = v[2 + end];
            out[4 + end] = v[4 + end];
            out
        };
        (first.plastic, second.plastic) = (shares(p, t, 0), shares(p, 1.0 - t, 1));
        (first.flow_rate, second.flow_rate) = (shares(r, t, 0), shares(r, 1.0 - t, 1));
        // Masses: each piece lumps half of its own onto its ends.
        let mass = el.mass();
        self.mass[a] -= 0.5 * mass * (1.0 - t);
        self.mass[b] -= 0.5 * mass * t;
        self.mass.push(0.5 * mass);
        self.inertia[a] += first.node_inertia() - el.node_inertia();
        self.inertia[b] += second.node_inertia() - el.node_inertia();
        self.inertia
            .push(first.node_inertia() + second.node_inertia());
        self.elements[e] = first;
        self.elements.push(second);
        m
    }

    /// Takes `other` in (its nodes after these, as it is now: moving, bent, loaded) and
    /// returns where its nodes start. The two are one structure, not yet joined.
    pub fn append(&mut self, other: &Self) -> NodeId {
        let offset = self.node_count();
        self.position.extend_from_slice(&other.position);
        self.orientation.extend_from_slice(&other.orientation);
        self.velocity.extend_from_slice(&other.velocity);
        self.angular_velocity
            .extend_from_slice(&other.angular_velocity);
        self.load.extend_from_slice(&other.load);
        self.torque.extend_from_slice(&other.torque);
        self.drag.extend_from_slice(&other.drag);
        self.rest.extend_from_slice(&other.rest);
        self.mass.extend_from_slice(&other.mass);
        self.inertia.extend_from_slice(&other.inertia);
        self.free_translation
            .extend_from_slice(&other.free_translation);
        self.free_rotation.extend_from_slice(&other.free_rotation);
        self.elements.extend(other.elements.iter().map(|e| {
            let mut e = *e;
            e.a += offset;
            e.b += offset;
            e
        }));
        offset
    }

    /// Joins nodes `a` and `b` (on two members, where the fasteners cross their axes) with a
    /// joint of law `law`, as they lie now: it holds them where they are. Returns its index.
    pub fn join(&mut self, a: NodeId, b: NodeId, law: &JointLaw) -> usize {
        let (pa, pb) = (self.position[a], self.position[b]);
        let d = pb - pa;
        let (section, material, kept) = law.equivalent(d.length());
        // Made in the world's frame, then carried into a's rest frame; b's frame is a's turned
        // by how the two nodes have turned apart since they were made.
        let mut joint = Element::new(
            (a, pa),
            (b, pb),
            d.any_orthonormal_vector(),
            section,
            material,
        );
        let (qa, qb) = (self.orientation[a], self.orientation[b]);
        let into_a = qa.conjugate();
        (joint.axis, joint.y, joint.z) = (into_a * joint.axis, into_a * joint.y, into_a * joint.z);
        joint.fastened = Some(super::fastened::Fastened {
            to_b: (qb.conjugate() * qa).normalize(),
            ..kept
        });
        self.elements.push(joint);
        self.elements.len() - 1
    }
}

impl Structure {
    /// Element `e` if it is a member (not a joint, not broken): its two nodes and its thickness
    /// (twice its section's least extent: what a fastener goes through).
    pub fn member(&self, e: usize) -> Option<(NodeId, NodeId, f64)> {
        let el = self.elements.get(e)?;
        (!el.is_joint() && !el.broken).then(|| {
            (
                el.a,
                el.b,
                2.0 * el.section.extent[0].min(el.section.extent[1]),
            )
        })
    }
}

impl Structure {
    /// How many of its members and joints are broken.
    pub fn elements_broken(&self) -> usize {
        self.elements.iter().filter(|e| e.broken).count()
    }
}

/// How far below a buried node its ground node lies, along the post (m): the soil spring's
/// length (its law sets its stiffness whatever the length).
const GROUND_GAP: f64 = 0.05;

impl Structure {
    /// Holds node `node` in the ground by a spring of law `law` along `axis` (unit: the
    /// post's way, out of the ground): a fixed node in the ground beside it, joined to it.
    /// Returns the joint's index.
    pub fn anchor(
        &mut self,
        node: NodeId,
        axis: creeer_math::glam::DVec3,
        law: &JointLaw,
    ) -> usize {
        let at = self.position[node] - axis * GROUND_GAP;
        self.fix_to(node, at, law)
    }

    /// Holds node `node` to the world at `at` by a joint of law `law`: a node held fast there,
    /// joined to it as the node lies now (a post's ground beside it; G2.9, a standing tree's
    /// axis, the tree taken as not bending). Returns the joint's index.
    pub fn fix_to(&mut self, node: NodeId, at: creeer_math::glam::DVec3, law: &JointLaw) -> usize {
        use creeer_math::glam::{DMat3, DVec3};
        let ground = self.node_count();
        self.position.push(at);
        self.orientation.push(DQuat::IDENTITY);
        self.velocity.push(DVec3::ZERO);
        self.angular_velocity.push(DVec3::ZERO);
        self.load.push(DVec3::ZERO);
        self.torque.push(DVec3::ZERO);
        self.drag.push(0.0);
        self.rest.push(at);
        // (Held fast: its mass is never moved, only kept positive for the matrix.)
        self.mass.push(1.0);
        self.inertia.push(DMat3::IDENTITY * 0.01);
        self.free_translation.push(DVec3::ZERO);
        self.free_rotation.push(DVec3::ZERO);
        self.join(ground, node, law)
    }
}
