//! Members that buckle: a member is split in two when its compression first reaches half
//! of what it can carry, and its halves split again if they need to.
//!
//! A member of one element bends only through its ends' rotations; the axial force acts on
//! that bending (the element's bowing, `element.rs`), but a member pinned at both ends, or
//! held at both ends by a stiff frame, cannot bend between them, and squashes at σ·A where a
//! real one would buckle first. Two elements buckle within 1 % of Euler's load (with the
//! bowing term; 24 % late without). Splitting every slender member up front would double
//! a frame's elements; most never carry much, so a member is split when its compression
//! first passes [`SPLIT`] of min(N_p, P_E), its squash load and its Euler load pinned at
//! both ends (members stockier than λ̄ = √(N_p/P_E) < 0.2 never buckle before they squash,
//! EN 1993-1-1 §6.3.1.2, and are never split).
//!
//! The split also gives the member its crookedness, L/1000 at mid-length along its weak
//! axis (the tolerance of EN 1090-2), in its rest shape as if it had always had it: a
//! perfectly straight member would not buckle at all. The crookedness is a half sine; two
//! straight halves carry it as a kink, sized to carry the half sine's first mode exactly
//! (π²/8 of its amplitude), which sets how the load amplifies it. The higher modes the kink
//! adds (0.23·e₀ at mid-length) the load hardly amplifies. The halves carry the member's
//! crookedness from then on and get none of their own: they are split again only to follow
//! the member's shape, when their compression passes [`SPLIT`] of their own Euler load.
//!
//! The new joint starts on the element's own cubic between its ends, turned to its tangent
//! there (each half's cubic is then the whole's), and is then settled with the member's
//! ends so that the forces on every one of them are what its motion needs (`settle.rs`): a
//! member split under a load it already carries is in equilibrium at once, kink and all.
//! Its mass is shared out so that the momentum stays; re-lumped, the member's kinetic energy
//! falls by m/16·|v_a − v_b|² (the lumped ends overstated it), nothing while it is slowly
//! compressed. Thin-walled sections also buckle locally (a tube's wall folds); that is not
//! modelled, nor are the residual stresses of rolling and welding.

use super::element::{Element, MEASURES};
use super::settle;
use super::{ImplicitSolver, NodeId, Structure};
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// The fraction of min(N_p, P_E) at which a member is split.
pub const SPLIT: f64 = 0.5;
/// Initial crookedness, per unit of length: a half sine of L/1000 at mid-length.
pub const CROOKEDNESS: f64 = 1.0e-3;
/// The kink two straight halves need at mid-length to carry that half sine's first mode.
pub const KINK: f64 = std::f64::consts::PI * std::f64::consts::PI / 8.0;
/// Relative slenderness λ̄ below which a member squashes before it buckles.
const STOCKY: f64 = 0.2;

impl Element {
    /// Whether a member with elastic measures `s` is compressed enough to be split.
    pub(crate) fn wants_split(&self, s: &[f64; MEASURES]) -> bool {
        let compression = -self.stiffness.apply(s)[0];
        if compression <= 0.0 || self.broken {
            return false;
        }
        let l = self.rest_length;
        let pi2 = std::f64::consts::PI * std::f64::consts::PI;
        let euler =
            pi2 * self.material.youngs_modulus * self.section.i_y.min(self.section.i_z) / (l * l);
        if self.crooked {
            return compression > SPLIT * euler;
        }
        let squash = self.material.flow_stress(self.plastic_strain) * self.section.area;
        squash >= STOCKY * STOCKY * euler && compression > SPLIT * squash.min(euler)
    }
}

impl Structure {
    /// Splits member `e` at mid-length (see the module docs; buckling does, and anything that
    /// needs a joint mid-member may): its first half takes its place, its second is appended.
    /// Returns the new joint.
    pub fn split_member(&mut self, e: usize) -> NodeId {
        let el = self.elements[e];
        let (a, b) = (el.a, el.b);
        let (qa, qb) = (self.orientation[a], self.orientation[b]);
        let (ta, tb) = (qa * el.axis, qb * el.axis);
        // The cubic through both ends with their tangents (Hermite): its middle, and its tangent
        // there (the ends' mean rotation turned onto it, which keeps their mean twist).
        let chord = self.position[b] - self.position[a];
        let l = chord.length();
        let middle = (self.position[a] + self.position[b]) * 0.5 + (ta - tb) * (l / 8.0);
        let tangent = (chord * 1.5 - (ta + tb) * (0.25 * l)).normalize();
        let mean = qa.slerp(qb, 0.5);
        let q = (DQuat::from_rotation_arc(mean * el.axis, tangent) * mean).normalize();
        // Crookedness along the weak axis, the way the member already bends (none for a half:
        // the member's is already in it).
        let weak = if el.section.i_y <= el.section.i_z {
            el.z
        } else {
            el.y
        };
        let bent = middle - (self.position[a] + self.position[b]) * 0.5;
        let side = if bent.dot(q * weak) < 0.0 {
            -weak
        } else {
            weak
        };
        let bow = if el.crooked {
            DVec3::ZERO
        } else {
            side * (KINK * CROOKEDNESS * el.rest_length)
        };
        let m = self.node_count();
        self.position.push(middle + q * bow);
        self.orientation.push(q);
        self.velocity
            .push((self.velocity[a] + self.velocity[b]) * 0.5);
        self.angular_velocity
            .push((self.angular_velocity[a] + self.angular_velocity[b]) * 0.5);
        self.load.push(DVec3::ZERO);
        self.torque.push(DVec3::ZERO);
        self.drag.push(0.0);
        self.rest.push((self.rest[a] + self.rest[b]) * 0.5 + bow);
        self.free_translation.push(DVec3::ONE);
        self.free_rotation.push(DVec3::ONE);
        // The halves, from the rest shape (nodes start unrotated).
        let half = |(i, pi): (usize, DVec3), (j, pj): (usize, DVec3)| {
            let mut h = Element::new((i, pi), (j, pj), el.y, el.section, el.material);
            h.plastic_strain = el.plastic_strain;
            h.crooked = true;
            // A hot member's halves are as hot: each is half as much longer.
            h.thermal = 0.5 * el.thermal;
            h
        };
        let mut first = half((a, self.rest[a]), (m, self.rest[m]));
        let mut second = half((m, self.rest[m]), (b, self.rest[b]));
        // Plastic deformation: the stretch and the twist shared, each end's bend kept at it.
        let p = el.plastic;
        first.plastic = [0.5 * p[0], 0.5 * p[1], p[2], 0.0, p[4], 0.0];
        second.plastic = [0.5 * p[0], 0.5 * p[1], 0.0, p[3], 0.0, p[5]];
        let r = el.flow_rate;
        first.flow_rate = [0.5 * r[0], 0.5 * r[1], r[2], 0.0, r[4], 0.0];
        second.flow_rate = [0.5 * r[0], 0.5 * r[1], 0.0, r[3], 0.0, r[5]];
        // Masses: each half lumps half of its own onto its ends.
        let quarter = 0.25 * el.mass();
        self.mass[a] -= quarter;
        self.mass[b] -= quarter;
        self.mass.push(2.0 * quarter);
        self.inertia[a] += first.node_inertia() - el.node_inertia();
        self.inertia[b] += second.node_inertia() - el.node_inertia();
        self.inertia
            .push(first.node_inertia() + second.node_inertia());
        self.elements[e] = first;
        self.elements.push(second);
        m
    }
}

impl ImplicitSolver {
    /// Splits the members of `s` that the last step found compressed enough (see the
    /// module docs) and rebuilds the solver for them, keeping its history; returns the new
    /// joints with the member each lies in (its radius for collisions).
    pub fn split_compressed(&mut self, s: &mut Structure) -> Vec<(NodeId, f64)> {
        if !s.splits || self.wanted.is_empty() {
            return Vec::new();
        }
        let mut wanted = std::mem::take(&mut self.wanted);
        wanted.sort_unstable();
        wanted.dedup();
        let old = s.node_count();
        let mut moved = vec![(DVec3::ZERO, DQuat::IDENTITY); old];
        let mut new = Vec::with_capacity(wanted.len());
        let mut parents = Vec::with_capacity(wanted.len());
        for e in wanted {
            let (a, b) = (s.elements[e].a, s.elements[e].b);
            let extent = s.elements[e].section.extent;
            let m = self.split_settled(s, e, &mut moved);
            new.push((m, extent[0].max(extent[1])));
            parents.push((a, b));
        }
        let mut solver = ImplicitSolver::new(
            s,
            self.scheme,
            self.h * f64::from(self.substeps),
            self.substeps,
        );
        // The history: kept for the old joints (moved with them), the mean of its member's
        // ends for a new one.
        let n = s.node_count();
        let parent = |i: usize| parents[i - old];
        let pick = |v: &Vec<DVec3>| -> Vec<DVec3> {
            (0..n)
                .map(|i| {
                    if i < old {
                        v[i]
                    } else {
                        let (a, b) = parent(i);
                        (v[a] + v[b]) * 0.5
                    }
                })
                .collect()
        };
        solver.previous_x = pick(&self.previous_x);
        solver.previous_v = pick(&self.previous_v);
        solver.previous_w = pick(&self.previous_w);
        solver.acceleration = pick(&self.acceleration);
        solver.angular_acceleration = pick(&self.angular_acceleration);
        // The new joints carry no load (`split`).
        let kept = |v: &[DVec3]| {
            (0..n)
                .map(|i| if i < old { v[i] } else { DVec3::ZERO })
                .collect::<Vec<_>>()
        };
        solver.previous_load = kept(&self.previous_load);
        solver.previous_torque = kept(&self.previous_torque);
        solver.previous_q = (0..n)
            .map(|i| {
                if i < old {
                    (moved[i].1 * self.previous_q[i]).normalize()
                } else {
                    let (a, b) = parent(i);
                    self.previous_q[a].slerp(self.previous_q[b], 0.5)
                }
            })
            .collect();
        for (x, &(shift, _)) in solver.previous_x.iter_mut().zip(&moved) {
            *x += shift;
        }
        // The new joint's previous position carries its offset from its chord too.
        for (k, &(m, _)) in new.iter().enumerate() {
            let (a, b) = parents[k];
            let offset = s.position[m] - (s.position[a] + s.position[b]) * 0.5;
            solver.previous_x[m] = (solver.previous_x[a] + solver.previous_x[b]) * 0.5 + offset;
        }
        solver.max_iterations = self.max_iterations;
        solver.tolerance = self.tolerance;
        solver.refactoring = self.refactoring;
        solver.energy_guard = self.energy_guard;
        solver.fallbacks = self.fallbacks;
        solver.factorisations = self.factorisations;
        solver.last_iterations = self.last_iterations;
        solver.last_yielding = self.last_yielding;
        *self = solver;
        new
    }

    /// Splits member `e` of `s` and settles the new joint and the member's ends (those held
    /// by other members or by supports: a free lone member's have nothing to settle against) so that
    /// every one moves on as before (`settle.rs`); adds what the old joints moved and turned
    /// to `moved`. Returns the new joint.
    fn split_settled(&self, s: &mut Structure, e: usize, moved: &mut [(DVec3, DQuat)]) -> NodeId {
        let (a, b) = (s.elements[e].a, s.elements[e].b);
        let held = |n: NodeId| {
            s.elements
                .iter()
                .enumerate()
                .any(|(i, x)| i != e && !x.broken && (x.a == n || x.b == n))
        };
        let ends: Vec<NodeId> = [a, b]
            .into_iter()
            .filter(|&n| held(n) || s.free_translation[n] != DVec3::ONE)
            .collect();
        let before = settle::gradients(s, &ends);
        let mass: Vec<(f64, DMat3)> = ends.iter().map(|&n| (s.mass[n], s.inertia[n])).collect();
        let m = s.split_member(e);
        let weight = |s: &Structure, n: NodeId| {
            s.gravity
                + s.planet
                    .as_ref()
                    .map_or(DVec3::ZERO, |p| p.acceleration(s.position[n]))
        };
        let world = |s: &Structure, n: NodeId, inertia: DMat3| {
            let r = DMat3::from_quat(s.orientation[n]);
            r * inertia * r.transpose()
        };
        // The new joint moves as its member's ends' mean.
        let (acceleration, turning) = (
            (self.acceleration[a] + self.acceleration[b]) * 0.5,
            (self.angular_acceleration[a] + self.angular_acceleration[b]) * 0.5,
        );
        let mut nodes = vec![m];
        let mut target = vec![[
            (weight(s, m) - acceleration) * s.mass[m],
            -(world(s, m, s.inertia[m]) * turning),
        ]];
        for (k, &n) in ends.iter().enumerate() {
            let (gained, inertia) = (s.mass[n] - mass[k].0, s.inertia[n] - mass[k].1);
            nodes.push(n);
            target.push([
                before[k][0] + (weight(s, n) - self.acceleration[n]) * gained,
                before[k][1] - world(s, n, inertia) * self.angular_acceleration[n],
            ]);
        }
        let turned: Vec<DQuat> = ends.iter().map(|&n| s.orientation[n]).collect();
        let shifts = settle::settle(s, &nodes, &target);
        for (k, &n) in ends.iter().enumerate() {
            let turn = s.orientation[n] * turned[k].conjugate();
            moved[n] = (moved[n].0 + shifts[k + 1], turn * moved[n].1);
        }
        m
    }
}
