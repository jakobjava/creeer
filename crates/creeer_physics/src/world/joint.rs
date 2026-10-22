//! Joints between bodies, or between a body and the world (G1.1, ADR-0022): a ball joint
//! (three rotations free) or a hinge (one), with limits on their angles, damping, friction
//! and a drive towards a target angle with a strength limit.
//!
//! A joint is an exact constraint of the step's minimisation, not a spring tuned to look
//! right: an augmented Lagrangian, its penalty the step's own stiffness (the lighter side's
//! mass over β², times [`STIFFER`]) and its multiplier carried from step to step and refreshed
//! where the minimisation converges (`Coupling::update_lag`), until the joint holds within
//! [`HOLDS`]. Limits are one-sided constraints of the same kind. Damping, friction and the
//! drive act on three points on the joint's frame axes, [`ARM`] from its centre, each pair
//! (one point on each side) apart only by the two sides' turn relative to each other: exact
//! and convex energy terms like the contacts' (`joint/terms.rs`).
//!
//! Bodies joined are stepped in one group, sleep and wake together, and do not collide with
//! each other unless the joint says so.

mod blocks;
mod group;
mod project;
mod rows;
mod save;
mod strut;
#[cfg(test)]
mod tests;

pub(crate) use group::Joints;

use super::World;
use crate::structure::NodeId;
use creeer_math::glam::{DQuat, DVec3};

/// The penalty's stiffness over the step's own (the lighter side's mass, or inertia, over
/// β²): each refresh of the multiplier leaves about 1/(1 + STIFFER) of the error before it.
pub const STIFFER: f64 = 100.0;
/// How far apart a joint's two points (m), or its axes (rad), may be when a step ends
/// (unless a refresh would barely change its force: [`SETTLED`]): a micrometre, a few of
/// the solver's own tolerance (3·10⁻⁷ m) its gaps settle within (at 10⁻⁷ every step of a
/// ragdoll took the three refreshes the minimisation allows, and their iterations).
pub const HOLDS: f64 = 1e-6;
/// A refresh of a joint's multipliers changing its force by less than this share of it
/// is not worth the minimisation it restarts.
pub const SETTLED: f64 = 0.01;
/// How far from the joint's centre its damping, friction and drive act (m).
pub const ARM: f64 = 0.1;

/// What a joint lets turn, and how far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Three rotations free: the angle between the two frames' axes up to `cone` (rad), and
    /// the twist about A's axis within `twist` (rad, from B's reference to A's).
    Ball {
        cone: Option<f64>,
        twist: Option<(f64, f64)>,
    },
    /// One rotation free, about the axis: its angle (B's reference from A's, about A's axis)
    /// within `range` (rad).
    Hinge { range: Option<(f64, f64)> },
}

/// A drive: B's frame turned towards `target` (relative to A's frame), with `stiffness`
/// (N·m/rad) and at most about `strength` (N·m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drive {
    /// B's frame relative to A's, as the drive wants it.
    pub target: DQuat,
    /// Its stiffness (N·m/rad).
    pub stiffness: f64,
    /// The most torque it gives (N·m; about: it is shared by its three arms).
    pub strength: f64,
}

/// The multipliers a joint carries from step to step: its constraint forces.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Multipliers {
    /// The force holding its two points together (N, on A).
    pub point: DVec3,
    /// What holds a hinge's axes together (N·m, along A's axis × B's).
    pub axis: DVec3,
    /// Its limits' torques (N·m): a hinge's low and high stop, or a ball's cone, low and high
    /// twist.
    pub limits: [f64; 3],
}

/// The force a joint applied over its last step (N, on A): its multiplier and its penalty on
/// the gap left at the step's end (G1.4f). Not the joint's state: not saved (none until the
/// next step) and not compared.
#[derive(Clone, Copy, Debug, Default)]
pub struct Applied(pub DVec3);

impl PartialEq for Applied {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Joint {
    /// The force with which side B holds side A at the joint, over the last step (N): what it
    /// applied, the multiplier with the penalty on the gap left (the multiplier alone lags it
    /// when the step's refreshes run out, as with a side moved every step).
    pub fn force(&self) -> DVec3 {
        -self.applied.0
    }

    /// The torque of limit `k` over the last step (N·m; order as `Multipliers::limits`).
    pub fn stop_torque(&self, k: usize) -> f64 {
        self.multipliers.limits[k]
    }
}

/// A joint.
#[derive(Clone, Debug, PartialEq)]
pub struct Joint {
    /// Side A: body and node.
    pub a: (usize, NodeId),
    /// Side B: body and node, or the world.
    pub b: Option<(usize, NodeId)>,
    /// Where it joins, in A's node frame.
    pub at_a: DVec3,
    /// Where it joins, in B's node frame (in the world, for the world).
    pub at_b: DVec3,
    /// Its frame in A's node frame: x its axis (the hinge's; the ball's twist axis), y the
    /// reference its angles are measured from.
    pub frame_a: DQuat,
    /// Its frame in B's node frame (in the world, for the world).
    pub frame_b: DQuat,
    /// What it lets turn.
    pub kind: Kind,
    /// Damping of the two sides' turn relative to each other (N·m·s/rad).
    pub damping: f64,
    /// Friction torque against that turn (N·m).
    pub friction: f64,
    /// A drive, if any.
    pub drive: Option<Drive>,
    /// Whether the two bodies collide with each other.
    pub collide: bool,
    /// Where its friction sticks: B's frame relative to A's (`None` until it first acts).
    pub anchor: Option<DQuat>,
    /// Its constraint forces, carried to the next step.
    pub multipliers: Multipliers,
    /// What it applied over the last step.
    pub applied: Applied,
    /// How fast its points move on their sides (m/s: `at_a` in A's node frame, `at_b` in B's
    /// or the world's), for whoever moves them each step (G1.4f: hands bringing a load in);
    /// its points' velocities are made to follow them. An input, not saved: none after a load.
    pub moving: [DVec3; 2],
}

impl Joint {
    /// A joint of `kind` between `a` and `b` (or the world) at the world point `at`, its
    /// axis `axis` and reference `reference` (perpendicular to it) in the world, the bodies
    /// as they are in `world` now: they are joined as they stand.
    ///
    /// # Panics
    /// If a side is not a body and node of `world`, or the axis and reference are not
    /// perpendicular.
    pub fn new(
        world: &World,
        (a, b): ((usize, NodeId), Option<(usize, NodeId)>),
        (at, axis, reference): (DVec3, DVec3, DVec3),
        kind: Kind,
    ) -> Self {
        let (axis, reference) = (axis.normalize(), reference.normalize());
        assert!(
            axis.dot(reference).abs() < 1e-9,
            "a joint's reference must be perpendicular to its axis"
        );
        let frame = frame_of(axis, reference);
        let local = |side: Option<(usize, NodeId)>| match side {
            Some((body, node)) => {
                let s = &world.bodies[body].structure;
                let q = s.orientation[node];
                (
                    q.conjugate() * (at - s.position[node]),
                    q.conjugate() * frame,
                )
            }
            None => (at, frame),
        };
        let ((at_a, frame_a), (at_b, frame_b)) = (local(Some(a)), local(b));
        Self {
            a,
            b,
            at_a,
            at_b,
            frame_a,
            frame_b,
            kind,
            damping: 0.0,
            friction: 0.0,
            drive: None,
            collide: false,
            anchor: None,
            multipliers: Multipliers::default(),
            applied: Applied::default(),
            moving: [DVec3::ZERO; 2],
        }
    }

    /// Whether it joins body `body`.
    pub fn joins(&self, body: usize) -> bool {
        self.a.0 == body || self.b.is_some_and(|b| b.0 == body)
    }
}

/// The rotation taking x to `axis` and y to `reference` (perpendicular unit vectors).
pub fn frame_of(axis: DVec3, reference: DVec3) -> DQuat {
    let z = axis.cross(reference);
    DQuat::from_mat3(&creeer_math::glam::DMat3::from_cols(axis, reference, z)).normalize()
}

impl World {
    /// Adds a joint; returns its index.
    ///
    /// # Panics
    /// If a side is not a body and node of the world, or both sides are one body.
    pub fn join(&mut self, joint: Joint) -> usize {
        let fits = |(body, node): (usize, NodeId)| {
            self.bodies
                .get(body)
                .is_some_and(|b| node < b.structure.node_count())
        };
        assert!(
            fits(joint.a) && joint.b.is_none_or(fits),
            "a joint on a body the world lacks"
        );
        assert!(
            joint.b.is_none_or(|b| b.0 != joint.a.0),
            "a joint within one body"
        );
        // Bodies joined wake together (one group from now on); joining them is an impulse,
        // which the next step's prediction must not carry on (as an impact's, `stepping.rs`).
        for body in [Some(joint.a.0), joint.b.map(|b| b.0)]
            .into_iter()
            .flatten()
        {
            let b = &mut self.bodies[body];
            b.wake();
            b.solver.reset_history(&b.structure);
        }
        self.joints.push(joint);
        self.joints.len() - 1
    }

    /// The joints.
    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }

    /// Joint `index`, to change its drive, damping or limits.
    pub fn joint_mut(&mut self, index: usize) -> &mut Joint {
        &mut self.joints[index]
    }

    /// Removes joint `index` (the joints after it move down by one) and returns it.
    pub fn remove_joint(&mut self, index: usize) -> Joint {
        let joint = self.joints.remove(index);
        self.bodies[joint.a.0].wake();
        if let Some((b, _)) = joint.b {
            self.bodies[b].wake();
        }
        joint
    }

    /// Pairs of bodies that do not collide (detection skips them): joined without colliding,
    /// or let pass ([`Self::let_pass`]).
    pub(super) fn unjoined_pairs(&self) -> Vec<(usize, usize)> {
        let mut pairs: Vec<(usize, usize)> = (self.joints.iter())
            .filter(|j| !j.collide)
            .filter_map(|j| j.b.map(|b| (j.a.0.min(b.0), j.a.0.max(b.0))))
            .chain(self.passing.iter().copied())
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    }

    /// Bodies `a` and `b` pass through each other from now on: no contacts between them, as
    /// if a joint kept them from colliding. Removing either ends it. Not saved
    /// ([`Self::to_bytes`]): whoever loads a world lets its pairs pass again.
    pub fn let_pass(&mut self, a: usize, b: usize) {
        let pair = (a.min(b), a.max(b));
        if let Err(at) = self.passing.binary_search(&pair) {
            self.passing.insert(at, pair);
        }
    }

    /// Whether bodies `a` and `b` were let pass through each other.
    pub fn passes(&self, a: usize, b: usize) -> bool {
        self.passing.binary_search(&(a.min(b), a.max(b))).is_ok()
    }

    /// Wakes every sleeping body joined to an awake one; returns whether any woke.
    pub(super) fn wake_joined(&mut self) -> bool {
        let mut woke = false;
        for j in 0..self.joints.len() {
            let Some((b, _)) = self.joints[j].b else {
                continue;
            };
            let a = self.joints[j].a.0;
            if self.bodies[a].sleep.asleep != self.bodies[b].sleep.asleep {
                self.bodies[a].wake();
                self.bodies[b].wake();
                woke = true;
            }
        }
        woke
    }

    /// The joints and the passing pairs renumbered to the bodies' new indices (`map`), those
    /// on a body removed dropped.
    pub(super) fn renumber_joints(&mut self, map: &[Option<usize>]) {
        let new = |body: usize| map.get(body).copied().flatten();
        let mut passing: Vec<(usize, usize)> = (self.passing.iter())
            .filter_map(|&(a, b)| Some((new(a)?, new(b)?)))
            .map(|(a, b)| (a.min(b), a.max(b)))
            .collect();
        passing.sort_unstable();
        self.passing = passing;
        let moved =
            |(body, node): (usize, NodeId)| map.get(body).copied().flatten().map(|b| (b, node));
        self.joints = std::mem::take(&mut self.joints)
            .into_iter()
            .filter_map(|mut j| {
                j.a = moved(j.a)?;
                j.b = match j.b {
                    Some(b) => Some(moved(b)?),
                    None => None,
                };
                Some(j)
            })
            .collect();
    }
}
