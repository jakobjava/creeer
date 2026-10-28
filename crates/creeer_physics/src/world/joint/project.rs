//! The joints' velocities made consistent at the end of a step: the least change, weighted
//! by the bodies' masses and inertias, that leaves every joint's points moving together (a
//! hinge's axes turning together, a stop not being run into further).
//!
//! The step holds a joint's points together; its velocities are the scheme's. Newmark takes
//! a node's velocity from its positions (v = 2·Δx/h − v₀): a velocity across a joint, once
//! there, changes sign every step and never dies away, and the joint's force rang with it,
//! low and high on alternate steps (a double pendulum at 60 Hz lost half its energy in three
//! seconds, its rejected steps redone dissipatively). Removing that velocity is the joint's
//! own velocity constraint (the time derivative of its position constraint), imposed as
//! equal and opposite impulses at the joint: momentum and angular momentum are kept, and
//! the energy taken is only the velocity the scheme made up.
//!
//! The impulses solve J·W·Jᵀ·μ = −J·v, its rows in blocks of three (a joint's point, its
//! hinge's axes, its stops being run into), each block coupled to those sharing a body: the
//! deterministic sparse Cholesky of the rigid bodies' joint solve (dense, a ragdoll's took
//! as long as the rest of its step).

use super::Joints;
use super::rows::{Row, Scalar};
use crate::sparse::{Cholesky, SymbolicCholesky};
use crate::structure::group::{Member, MemberState};
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// A limit counts as reached within this (rad).
const AT_STOP: f64 = 1e-6;
/// The relative regularisation of the solve (rows of a hinge's axes are of rank two).
const REGULAR: f64 = 1e-12;

/// A side's inverse mass and world inverse inertia (fixed axes masked).
fn inverse(member: &Member<'_>, node: usize) -> (DVec3, DMat3) {
    let s = &member.0;
    let mass = s.masses()[node];
    let t = s.free_translation[node] / mass;
    let r = DMat3::from_quat(s.orientation[node]);
    let inertia = s.inertia[node];
    let i = if inertia.determinant().abs() > 0.0 {
        r * inertia.inverse() * r.transpose()
    } else {
        DMat3::ZERO
    };
    let mask = DMat3::from_diagonal(s.free_rotation[node]);
    (t, mask * i * mask)
}

fn apply(row: &Row, (t, i): (DVec3, DMat3)) -> [f64; 6] {
    let lin = DVec3::new(row[0], row[1], row[2]) * t;
    let ang = i * DVec3::new(row[3], row[4], row[5]);
    [lin.x, lin.y, lin.z, ang.x, ang.y, ang.z]
}

fn dot(a: &Row, b: &[f64; 6]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A block of up to three velocity constraints of one joint, on its sides.
struct Block {
    rows: [Option<Scalar>; 3],
    sides: [Option<(usize, usize)>; 2],
    /// W·rowᵀ of each row on each side.
    weighted: [[[f64; 6]; 2]; 3],
    at: DVec3,
    /// The rate its rows are to have (the joint's points moving on their sides: m/s).
    target: DVec3,
    /// A joint's stops (one-sided: held only while run into).
    stop: bool,
}

/// Rate of block rows' constraints at the members' velocities.
fn rates(block: &Block, members: &[Member<'_>]) -> DVec3 {
    let mut out = DVec3::ZERO;
    for (k, row) in block.rows.iter().enumerate() {
        let Some(s) = row else { continue };
        for (side, r) in block.sides.iter().zip([&s.a, &s.b]) {
            if let Some((m, n)) = side {
                let st = &members[*m].0;
                let (v, w) = (st.velocity[*n], st.angular_velocity[*n]);
                out[k] += dot(r, &[v.x, v.y, v.z, w.x, w.y, w.z]);
            }
        }
    }
    out
}

/// The 3×3 block of K = J·W·Jᵀ between blocks `x` and `y` over the sides they share.
fn coupling(x: &Block, y: &Block) -> DMat3 {
    let mut k = [[0.0; 3]; 3];
    for (sx, side) in x.sides.iter().enumerate() {
        let Some(side) = side else { continue };
        for (sy, other) in y.sides.iter().enumerate() {
            if other.as_ref() != Some(side) {
                continue;
            }
            for (i, ri) in x.rows.iter().enumerate() {
                let Some(ri) = ri else { continue };
                let ri = if sx == 0 { &ri.a } else { &ri.b };
                for (j, w) in y.weighted.iter().enumerate() {
                    if y.rows[j].is_some() {
                        k[j][i] += dot(ri, &w[sy]);
                    }
                }
            }
        }
    }
    DMat3::from_cols_array_2d(&k)
}

impl Joints {
    /// Makes the members' velocities consistent with the joints (module docs), and moves
    /// friction's anchors that slipped.
    pub(crate) fn project(&mut self, members: &mut [Member<'_>]) {
        if self.list.is_empty() {
            // A grip alone (a thing held in the air): the hands' work booked all the same.
            if self.has_grips() {
                let states: Vec<MemberState<'_>> = (members.iter())
                    .map(|(s, _)| MemberState {
                        position: &s.position,
                        orientation: &s.orientation,
                    })
                    .collect();
                self.driven = self.grip_work(&states);
            }
            return;
        }
        let timer = crate::profile::start();
        self.record_applied(members);
        self.follow_anchors(members);
        self.project_velocities(members);
        timer.stop(crate::profile::Slot::JointProjection);
    }

    /// Each joint's force over the step: its multiplier and its penalty on the gap left where
    /// the members ended (the gradient its point terms gave the step's minimisation).
    /// And the work its moved points did: the force times how far their moving opened the gap
    /// over the substep (an energy the members' does not count: unbooked, the energy guard took
    /// a load lifted by moving a joint's point for the step's gain and stopped it, G1.4f).
    fn record_applied(&mut self, members: &[Member<'_>]) {
        let states: Vec<MemberState<'_>> = (members.iter())
            .map(|(s, _)| MemberState {
                position: &s.position,
                orientation: &s.orientation,
            })
            .collect();
        let h = self.h;
        self.driven = 0.0;
        for (j, (_, joint)) in self.list.iter_mut().enumerate() {
            let at = joint.measured(&states, false);
            let (kt, _) = self.stiffness.get(j).copied().unwrap_or_default();
            let mut force = joint.multipliers.point;
            for (k, s) in at.point.iter().enumerate() {
                force[k] += kt * s.value;
            }
            joint.applied = super::Applied(force);
            if joint.moving != [DVec3::ZERO; 2] {
                let turn = |side: Option<(usize, usize)>| {
                    side.map_or(DQuat::IDENTITY, |(m, n)| states[m].orientation[n])
                };
                let opened =
                    turn(Some(joint.a)) * joint.moving[0] - turn(joint.b) * joint.moving[1];
                self.driven += force.dot(opened) * h;
            }
        }
        // The hands' work on what they grip (`grip.rs`), as a moved point's.
        self.driven += self.grip_work(&states);
    }

    /// Whether any joint's points are moved, or anything is gripped.
    pub(crate) fn driven(&self) -> bool {
        self.list.iter().any(|(_, j)| j.moving != [DVec3::ZERO; 2]) || self.has_grips()
    }

    /// The work the moved points did over the last substep (J; the end's force: a joint's terms
    /// are potentials at the step's end, not loads Newmark takes the mean of).
    pub(crate) fn driven_work(&self) -> f64 {
        self.driven
    }

    /// Friction's anchor follows a slip beyond its stick, kept that far behind (as the
    /// contacts' anchors are, `world/anchor.rs`): a load below the friction never creeps.
    fn follow_anchors(&mut self, members: &[Member<'_>]) {
        let states: Vec<MemberState<'_>> = (members.iter())
            .map(|(s, _)| MemberState {
                position: &s.position,
                orientation: &s.orientation,
            })
            .collect();
        for (_, joint) in &mut self.list {
            if joint.anchor.is_none() {
                continue;
            }
            let (Some(anchor), Some(arms)) = (joint.anchor, joint.measured(&states, false).stick)
            else {
                continue;
            };
            let slip = (arms.iter().flatten())
                .map(|s| s.value * s.value)
                .sum::<f64>()
                .sqrt();
            if slip > super::group::STICK {
                let now = joint.relative(&states);
                joint.anchor = Some(now.slerp(anchor, super::group::STICK / slip).normalize());
            }
        }
    }

    fn project_velocities(&self, members: &mut [Member<'_>]) {
        let mut blocks: Vec<Block> = Vec::new();
        {
            let states: Vec<MemberState<'_>> = (members.iter())
                .map(|(s, _)| MemberState {
                    position: &s.position,
                    orientation: &s.orientation,
                })
                .collect();
            for (_, joint) in &self.list {
                let at = joint.at(&states);
                let sides = [Some(joint.a), joint.b];
                let centre = states[joint.a.0].position[joint.a.1]
                    + states[joint.a.0].orientation[joint.a.1] * joint.at_a;
                let block = |rows: [Option<Scalar>; 3], stop: bool| Block {
                    rows,
                    sides,
                    weighted: [[[0.0; 6]; 2]; 3],
                    at: centre,
                    target: DVec3::ZERO,
                    stop,
                };
                // Its points moved on their sides: the bodies' points move against it.
                let turn = |side: Option<(usize, usize)>| {
                    side.map_or(DQuat::IDENTITY, |(m, n)| states[m].orientation[n])
                };
                let moving =
                    turn(Some(joint.a)) * joint.moving[0] - turn(joint.b) * joint.moving[1];
                blocks.push(Block {
                    target: -moving,
                    ..block(at.point.map(Some), false)
                });
                if let Some(axis) = at.axis {
                    blocks.push(block(axis.map(Some), false));
                }
                // The stops reached (being run into is checked below, at the velocities).
                let mut stops = at.limits.map(|s| s.filter(|s| s.value > -AT_STOP));
                if stops.iter().any(Option::is_some) {
                    stops.sort_by_key(Option::is_none);
                    blocks.push(block(stops, true));
                }
            }
        }
        // A stop is held only while it is being run into.
        for block in blocks.iter_mut().filter(|b| b.stop) {
            let rate = rates(block, members);
            for (k, row) in block.rows.iter_mut().enumerate() {
                if row.is_some() && rate[k] <= 0.0 {
                    *row = None;
                }
            }
        }
        blocks.retain(|b| b.rows.iter().any(Option::is_some));
        if blocks.is_empty() {
            return;
        }
        for block in &mut blocks {
            for (k, row) in block.rows.iter().enumerate() {
                let Some(s) = row else { continue };
                for (side, (w, r)) in block
                    .sides
                    .iter()
                    .zip(block.weighted[k].iter_mut().zip([&s.a, &s.b]))
                {
                    if let Some((m, n)) = side {
                        *w = apply(r, inverse(&members[*m], *n));
                    }
                }
            }
        }
        // Blocks sharing a body are coupled.
        let mut edges = Vec::new();
        for x in 0..blocks.len() {
            for y in x + 1..blocks.len() {
                let shares = (blocks[x].sides.iter().flatten())
                    .any(|s| blocks[y].sides.iter().flatten().any(|t| t == s));
                if shares {
                    edges.push((x, y));
                }
            }
        }
        let mut diag: Vec<DMat3> = blocks.iter().map(|b| coupling(b, b)).collect();
        let scale = (diag.iter())
            .map(|d| d.x_axis.x.max(d.y_axis.y).max(d.z_axis.z))
            .fold(0.0, f64::max)
            .max(f64::MIN_POSITIVE);
        for (d, b) in diag.iter_mut().zip(&blocks) {
            for (k, row) in b.rows.iter().enumerate() {
                // A row without a constraint: identity, solved for nought.
                let add = if row.is_some() { REGULAR * scale } else { 1.0 };
                *d.col_mut(k) += DVec3::AXES[k] * add;
            }
        }
        let off: Vec<DMat3> = edges
            .iter()
            .map(|&(x, y)| coupling(&blocks[x], &blocks[y]))
            .collect();
        let positions: Vec<DVec3> = blocks.iter().map(|b| b.at).collect();
        let mut chol = Cholesky::new(SymbolicCholesky::new(&positions, &edges));
        if chol.factor(&diag, &off).is_err() {
            return;
        }
        let mut mu: Vec<DVec3> = (blocks.iter())
            // (Exactly −rate where nothing moves: bit-identical to the joints before G1.4f.)
            .map(|b| {
                if b.target == DVec3::ZERO {
                    -rates(b, members)
                } else {
                    b.target - rates(b, members)
                }
            })
            .collect();
        chol.solve(&mut mu);
        // The impulses, through each side's inverse mass.
        for (b, m) in blocks.iter().zip(&mu) {
            for (k, row) in b.rows.iter().enumerate() {
                if row.is_none() {
                    continue;
                }
                for (side, w) in b.sides.iter().zip(&b.weighted[k]) {
                    let Some((member, node)) = side else { continue };
                    let s = &mut members[*member].0;
                    s.velocity[*node] += DVec3::new(w[0], w[1], w[2]) * m[k];
                    s.angular_velocity[*node] += DVec3::new(w[3], w[4], w[5]) * m[k];
                }
            }
        }
    }
}
