//! The arms reaching what the hands hold (G2.10, ADR-0036): two-bone inverse kinematics, as
//! Unreal's and Unity's two-bone IK do it (an effector and a joint target), solved exactly as
//! the legs are (`skeleton::Chain`). The hand is turned first to hold as it means to
//! (`grasp.rs`), which puts the wrist where that hand needs it; the elbow bends until the wrist
//! is that far from the shoulder, and the arm turns at the shoulder to point there, the elbow
//! towards a pole down, back and a little out from the line (as people's elbows hang: a reach
//! forward points them down, one down at the sides back). Beyond the arm's reach (a load lagging
//! or torn away) it points at the wrist as straight as it goes, the hand on its end turned as it
//! would hold: nothing stretches, nothing turns over.

use super::skeleton::{Chain, ELBOW, SHOULDER, limb};
use creeer_math::Frame64;
use creeer_math::glam::DVec3;
use creeer_sim::character::bones;

/// The elbow's most bend from straight (rad): the ragdoll's elbow's range (`ragdoll::LINKS`).
const ELBOW_MOST: f64 = 2.5;
/// The elbow's pole, from the shoulder: back, out and down, in these shares.
const POLE_BACK: f64 = 1.0;
const POLE_OUT: f64 = 0.3;
const POLE_DOWN: f64 = 1.0;

/// Arm `side` (0 the right) placed so its hand's hold frame `rest` (at rest:
/// `HandShape::rest`) lies at `held` (the world), from its shoulder where the drawn `torso` has
/// it, the body upright along `up` and facing `forward`. Returns its upper arm's, forearm's
/// and hand's bones, and how far short of `held` the hand stops (m: beyond its reach).
pub fn arm(
    side: usize,
    torso: &Frame64,
    (rest, held): (Frame64, Frame64),
    up: DVec3,
    forward: DVec3,
) -> ([Frame64; 3], f64) {
    let joints = bones::joints();
    let (s, e, w) = (
        joints[SHOULDER[side]].2,
        joints[ELBOW[side]].2,
        joints[ELBOW[side] + 1].2,
    );
    let shoulder = torso.transform_point(s);
    // The hand as it holds, and so where its wrist is.
    let turn = (held.rotation * rest.rotation.inverse()).normalize();
    let hand = Frame64::new(held.position - turn * rest.position, turn);
    let wrist = hand.transform_point(w);
    let chain = Chain::new(s, e, w);
    let off = chain.off_straight((wrist - shoulder).length());
    // Flexed forward from straight (a turn about −x, as `upper.rs` bends the elbow).
    let bend = chain.straightest - off.min(ELBOW_MOST);
    let left = up.cross(forward).normalize_or(DVec3::X);
    let out = if side == 0 { -left } else { left };
    let pole = -forward * POLE_BACK + out * POLE_OUT - up * POLE_DOWN;
    let way = wrist - shoulder;
    // The elbow's hinge across the plane of the arm and its pole (`Chain`'s x).
    let hinge = way.cross(pole).try_normalize().unwrap_or(left);
    let ([upper, fore], reached) = limb([s, e, w], bend, (shoulder, wrist), hinge, -up);
    let hand = Frame64::new(reached - turn * w, turn);
    ([upper, fore, hand], (wrist - reached).length())
}

/// How far arm `side` reaches from its shoulder to its hand's hold frame `rest` (at rest:
/// `HandShape::rest`), as straight as it goes (m).
pub fn arm_reach(side: usize, rest: Frame64) -> f64 {
    let joints = bones::joints();
    let (s, e, w) = (
        joints[SHOULDER[side]].2,
        joints[ELBOW[side]].2,
        joints[ELBOW[side] + 1].2,
    );
    let chain = Chain::new(s, e, w);
    chain.span(chain.straightest) + (rest.position - w).length()
}

#[cfg(test)]
#[path = "arms_tests.rs"]
mod tests;
