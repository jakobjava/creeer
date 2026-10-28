//! Grips in a group's minimisation (`world/grip.rs`), beside its joints and struts: the held
//! point and the node's three arms with their rows, the energy, gradient and Gauss–Newton
//! blocks, and the hands' work for the energy guard.
//!
//! In the step the point is a spring of K = k + c/β towards r* = (k·target + (c/β)·(p̂ +
//! β·v_target))/K: its damper acts on the point's velocity relative to the target's, the
//! point's velocity being (p − p̂)/β. A vector Huber spring on u = p − r*: ½K|u|² within the
//! cap, most·|u| − most²/2K beyond it, convex. Its Gauss–Newton blocks are exact: K·I within
//! the cap, (most/|u|)·(I − n·nᵀ) beyond it, given as three scalar rows along n and two
//! directions across it. The arms alike, with rows on the turn alone.

use super::group::Joints;
use super::rows::{Row, Scalar};
use crate::structure::group::{MemberState, StepInfo};
use crate::world::grip::Grip;
use creeer_math::glam::{DQuat, DVec3};

/// A grip of a group: its side numbered as the group's members, whether it is a rigid body,
/// and its point and arms at x̂ (the damping's reference) and at the start (the work's).
#[derive(Clone, Debug)]
pub(crate) struct GroupGrip {
    pub grip: Grip,
    pub rigid: bool,
    hat: Option<Pose>,
    start: Option<Pose>,
}

/// The held point (world), its arm from the node, and the node's arms.
#[derive(Clone, Copy, Debug)]
struct Pose {
    point: DVec3,
    arm: DVec3,
    arms: [DVec3; 3],
}

fn row(translation: DVec3, turn: DVec3) -> Row {
    [
        translation.x,
        translation.y,
        translation.z,
        turn.x,
        turn.y,
        turn.z,
    ]
}

fn measured(g: &Grip, states: &[MemberState<'_>]) -> Pose {
    let (m, n) = g.a;
    let (x, q): (DVec3, DQuat) = (states[m].position[n], states[m].orientation[n]);
    let arm = q * g.at_a;
    Pose {
        point: x + arm,
        arm,
        arms: Grip::arms(q),
    }
}

/// A vector Huber spring at `u` with stiffness `k`, its pull at most `most`: its energy, and
/// its terms as (direction, derivative along it, weight), the directions orthonormal.
fn huber(u: DVec3, k: f64, most: f64) -> (f64, [(DVec3, f64, f64); 3]) {
    let r = u.length();
    if k * r <= most || r == 0.0 {
        let terms = [DVec3::X, DVec3::Y, DVec3::Z].map(|e| (e, k * u.dot(e), k));
        return (0.5 * k * r * r, terms);
    }
    let n = u / r;
    let t = n.any_orthonormal_vector();
    let across = most / r;
    (
        most * r - 0.5 * most * most / k,
        [(n, most, 0.0), (t, 0.0, across), (n.cross(t), 0.0, across)],
    )
}

/// A spring's energy and its terms: (direction, derivative along it, weight).
type Spring = (f64, [(DVec3, f64, f64); 3]);

/// Grip `g`'s four springs at pose `at` in a step of `beta`: its point's, then each arm's.
fn springs(g: &GroupGrip, at: &Pose, beta: f64) -> [Spring; 4] {
    let grip = &g.grip;
    // Each spring's rest and stiffness in the step: its damper as a shifted rest.
    let shifted = |k: f64, c: f64, want: DVec3, hat: Option<DVec3>, moves: DVec3| {
        hat.map_or((want, k), |hat| {
            let c = c / beta;
            ((want * k + (hat + moves * beta) * c) / (k + c), k + c)
        })
    };
    let (rest, stiff) = shifted(
        grip.stiffness,
        grip.damping,
        grip.target,
        g.hat.map(|h| h.point),
        grip.target_velocity,
    );
    let wants = Grip::arms(grip.turn);
    let arm = |i: usize| {
        let (rest, stiff) = shifted(
            grip.turn_stiffness,
            grip.turn_damping,
            wants[i],
            g.hat.map(|h| h.arms[i]),
            grip.spin.cross(wants[i]),
        );
        huber(at.arms[i] - rest, stiff, grip.most_arm())
    };
    [
        huber(at.point - rest, stiff, grip.most),
        arm(0),
        arm(1),
        arm(2),
    ]
}

impl Joints {
    /// Adds a grip (its side numbered as the group's members; `rigid`: whether it is a rigid
    /// body).
    pub(crate) fn add_grip(&mut self, grip: Grip, rigid: bool) {
        self.grips.push(GroupGrip {
            grip,
            rigid,
            hat: None,
            start: None,
        });
    }

    /// Whether the group has grips.
    pub(crate) fn has_grips(&self) -> bool {
        !self.grips.is_empty()
    }

    /// Sets the grips' step up: their poses at x̂ and at the start.
    pub(super) fn begin_grips(&mut self, info: &StepInfo<'_>) {
        for g in &mut self.grips {
            g.hat = Some(measured(&g.grip, info.hat));
            g.start = Some(measured(&g.grip, info.start));
        }
    }

    /// Each grip's terms at `states`: (grip, a scalar with its row, the energy's derivative
    /// by it, its weight); the energy in all. Before the step is set up, its springs alone.
    pub(super) fn grip_terms(
        &self,
        states: &[MemberState<'_>],
        mut each: impl FnMut(usize, &Scalar, f64, f64),
    ) -> f64 {
        let mut total = 0.0;
        for (k, g) in self.grips.iter().enumerate() {
            let at = measured(&g.grip, states);
            for (s, (energy, terms)) in springs(g, &at, self.beta()).iter().enumerate() {
                total += energy;
                // The point's rows move with the node and turn about it; an arm's turn only.
                let (moved, from) = match s {
                    0 => (true, at.arm),
                    _ => (false, at.arms[s - 1]),
                };
                for &(d, derivative, weight) in terms {
                    let scalar = Scalar {
                        value: 0.0,
                        a: row(if moved { d } else { DVec3::ZERO }, from.cross(d)),
                        b: [0.0; 6],
                    };
                    each(k, &scalar, derivative, weight);
                }
            }
        }
        total
    }

    /// Grip `k`'s side and whether it is a rigid body.
    pub(super) fn grip_side(&self, k: usize) -> ((usize, usize), bool) {
        let g = &self.grips[k];
        (g.grip.a, g.rigid)
    }

    /// The energy the grips' springs store at `states` (J): each towards its target, which
    /// stands still within a step (k alone: the damper stores none), capped as it pulls.
    pub(super) fn grip_stored(&self, states: &[MemberState<'_>]) -> f64 {
        let mut stored = 0.0;
        for g in &self.grips {
            let (grip, at) = (&g.grip, measured(&g.grip, states));
            stored += huber(at.point - grip.target, grip.stiffness, grip.most).0;
            for (own, want) in at.arms.into_iter().zip(Grip::arms(grip.turn)) {
                stored += huber(own - want, grip.turn_stiffness, grip.most_arm()).0;
            }
        }
        stored
    }

    /// The work the hands did on the held bodies over the substep that ended at `states`
    /// (J). Within a step the target stands still and the spring is a potential (stored, as
    /// a strut's); what the hands give beyond it is their damper's drag at the target's own
    /// speed, c·v_target over the way the point went (and the arms' alike). (Booked as the
    /// whole pull over the way, a 1 kg box raised 0.22 m had six of its steps redone.)
    pub(super) fn grip_work(&self, states: &[MemberState<'_>]) -> f64 {
        let mut work = 0.0;
        for g in &self.grips {
            let Some(start) = g.start else { continue };
            let (grip, end) = (&g.grip, measured(&g.grip, states));
            work += (grip.target_velocity * grip.damping).dot(end.point - start.point);
            for (i, want) in Grip::arms(grip.turn).into_iter().enumerate() {
                let drag = grip.spin.cross(want) * grip.turn_damping;
                work += drag.dot(end.arms[i] - start.arms[i]);
            }
        }
        work
    }
}
