//! The hips (G1.3g; G1.4a): lowered as far as the farther foot needs (Johansen 2009), stepping
//! with the knee never quite straight (`skeleton::stance_leg`), standing as straight as it goes.
//! A foot in the air need not be reached until it lands: through its swing it asks for more and
//! more of what it will need landing, from the hips as they will be then, so the hips go down on
//! a smooth path (Saunders, Inman and Eberhart 1953), not all at once as it meets the ground; as
//! a foot leaves the ground, they come back up as a mass does.

use super::feet::{self, Through};
use super::{State, skeleton};
use creeer_math::Frame64;
use creeer_math::glam::DVec3;
use creeer_sim::character::bones;

/// How fast the hips come back up when no foot holds them down (1/s): critically damped, as
/// fast as lets the knee straighten through a walking stance to about 5° as people's does
/// (Perry: the stance knee 15–20° at its most bent, about 5° at mid-stance).
const HIPS: f64 = 40.0;

/// The body's motion as the feet are placed: its up, the way it faces, its velocity along the
/// ground (m/s) and its acceleration the way it faces (m/s²).
pub(super) struct Going {
    pub up: DVec3,
    pub forward: DVec3,
    pub along: DVec3,
    pub ahead: f64,
}

/// A foot as the hips see it: its ankle now, where it is through its step, where its swing
/// is aimed (its point on the ground and the normal there) and how long until it lands (s).
pub(super) struct Reached {
    pub ankle: DVec3,
    pub through: Through,
    pub aimed: (DVec3, DVec3),
    pub lands_in: f64,
}

/// How far the hips in `root` must be lowered for the feet (m, none if they reach), `stepping`
/// or standing.
pub(super) fn needed(root: &Frame64, going: &Going, feet: &[Reached; 2], stepping: bool) -> f64 {
    let joints = bones::joints();
    let reach = if stepping {
        skeleton::stance_leg()
    } else {
        skeleton::longest_leg()
    };
    let up = going.up;
    let need = |side: usize, ankle: DVec3, later: f64| {
        let hip = root.transform_point(joints[skeleton::HIP[side]].2)
            + going.along * later
            + going.forward * (0.5 * going.ahead * later * later);
        let to = ankle - hip;
        let down = -to.dot(up);
        let across = (to + up * down).length();
        down - (reach * reach - across * across).max(0.0).sqrt()
    };
    let mut drop = 0.0f64;
    for (side, foot) in feet.iter().enumerate() {
        if let (true, Through::Swing(s)) = (stepping, foot.through) {
            let (g, n) = foot.aimed;
            let (lands, _) = feet::rolled(side, g, n, going.forward, Through::Stance(0.0), false);
            let then = need(side, lands, foot.lands_in);
            let ramp = s * s * (3.0 - 2.0 * s);
            // And reached now with a stance's bent knee as it comes down (reaching straight to the
            // last, the knee bent 13° in the frame it landed: near straight, a knee's bend goes as
            // the square root of the leg's shortfall).
            drop = drop
                .max(then * ramp)
                .max(need(side, foot.ankle, 0.0) * ramp);
        } else {
            drop = drop.max(need(side, foot.ankle, 0.0));
        }
    }
    drop
}

impl State {
    /// The hips lowered `needed` (m) this frame, `dt` after the last: at once as far as a foot
    /// needs (it must be reached); coming back up, they follow as a mass does, critically
    /// damped, so their speed never jumps. A stance just begun (`fresh`) starts where it needs
    /// them, still (the pose smooths the change).
    pub(super) fn lower_hips(&mut self, needed: f64, fresh: bool, dt: f64) {
        let was = self.drop;
        // The spring stepped exactly (the same at any frame rate): off by e at speed v, after t
        // it is off by (e + (v + ωe)t)·e^(−ωt), at speed (v − ω(v + ωe)t)·e^(−ωt).
        let (e, v) = (self.drop - needed, self.lowering);
        let (c, decay) = (v + HIPS * e, creeer_math::det::exp(-HIPS * dt));
        self.drop = needed + (e + c * dt) * decay;
        self.lowering = (v - HIPS * c * dt) * decay;
        if fresh {
            (self.drop, self.lowering) = (needed, 0.0);
        } else if self.drop < needed {
            self.drop = needed;
            self.lowering = if dt > 0.0 { (needed - was) / dt } else { 0.0 };
        }
    }
}
