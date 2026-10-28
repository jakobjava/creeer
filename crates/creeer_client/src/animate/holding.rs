//! Holding a load as drawn (G2.10, ADR-0036): standing, the body turns to face what it holds;
//! it stoops and squats to it when it is held low, as the server's hands assume
//! (`reach::stooped`: the shoulders down by up to 0.7 m and on by 0.35 m), the trunk leaning at
//! the hips and the waist and the hips lowered (the legs' inverse kinematics keep the feet on
//! the ground); and the arms reach where the hands hold it (`grasp.rs`, `arms.rs`), coming to it
//! as a hand takes hold and going back from it as it lets go.

use super::State;
use super::arms::arm_reach;
use super::feet::Ground;
use super::grasp::{Load, Reaching};
use super::hands::Hold;
use super::skeleton::{self, ELBOW, SHOULDER, about, then};
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::character::{self, bones};
use creeer_sim::reach;

/// How long the arms take to reach out to a load and back from it (s): the hands from hanging
/// to before the body (about 0.4 m) along a smooth step, at most 1.7 m/s (peak hand speeds in
/// reaching 1.5–2 m/s).
const REACHING: f64 = 0.35;
/// How far above the ground a box's bottom must be for the fingers to get under it (m).
const CLEAR: f64 = 0.03;
/// How fast the hands slide to a new hold on what they hold (m/s, rad/s): faster than they move
/// along a pole hand over hand (0.5 m/s), round to a box's other face in about a third of a
/// second.
const SLIDE: f64 = 1.0;
const SLIDE_TURN: f64 = 8.0;
/// Standing, the body turns to face what it holds when it is more than this off its front (rad,
/// 30°: within it the arms reach aside to it), as fast as it turns walking (rad/s).
const FACING: f64 = 0.52;
const TURN: f64 = 8.0;
/// How fast the body stoops and straightens (1/s): critically damped, in about a third of a
/// second.
const STOOPING: f64 = 12.0;
/// The share of a stoop's lean bent at the waist (the rest at the hips: a back kept fairly
/// straight, as most lifts are), the most it leans (rad: 50°), and the share of it the head
/// looks down by.
const BACK_SHARE: f64 = 0.3;
const MOST_STOOP: f64 = 0.87;
const LOOK_DOWN: f64 = 0.5;
/// How near the hips come to the ankles squatting, beyond the legs' shortest (m).
const SQUAT_SPARE: f64 = 0.03;
/// The share of an arm's full reach it holds at easily (the elbow still bent, about 50°): held
/// farther below, the body stoops.
const EASY: f64 = 0.9;

impl State {
    /// Standing still holding something (`centre`, upright along `up`): turns towards it, at
    /// most [`TURN`] a second, until it is within [`FACING`] of the front.
    pub(super) fn face_held(&mut self, up: DVec3, dt: f64) {
        let Some(load) = self.held.filter(|_| self.holding) else {
            return;
        };
        let Some(want) = (load.held() - self.centre).reject_from(up).try_normalize() else {
            return;
        };
        let angle = self.heading.angle_between(want);
        if angle > FACING {
            let sign = up.dot(self.heading.cross(want)).signum();
            let step = (angle - FACING).min(TURN * dt) * sign;
            self.heading = (DQuat::from_axis_angle(up, step) * self.heading).normalize();
        }
    }

    /// The stoop to what is held low this frame (`turns` given their bend at the waist and the
    /// head's look down): the lean at the hips (rad) and how far the hips are lowered (m).
    pub(super) fn stoop(
        &mut self,
        turns: &mut [DQuat; 14],
        up: DVec3,
        forward: DVec3,
        dt: f64,
    ) -> (f64, f64) {
        let (lean, drop) = match self.held.filter(|_| self.holding) {
            Some(load) => self.stoop_needed(&load, up, forward),
            None => (0.0, 0.0),
        };
        // Each followed critically damped, stepped exactly (as `hips.rs`).
        for (k, want) in [lean, drop].into_iter().enumerate() {
            let (e, v) = (self.stoop[2 * k] - want, self.stoop[2 * k + 1]);
            let (c, decay) = (v + STOOPING * e, creeer_math::det::exp(-STOOPING * dt));
            self.stoop[2 * k] = want + (e + c * dt) * decay;
            self.stoop[2 * k + 1] = (v - STOOPING * c * dt) * decay;
        }
        let lean = self.stoop[0].max(0.0);
        turns[0] = DQuat::from_rotation_x(BACK_SHARE * lean) * turns[0];
        turns[1] = DQuat::from_rotation_x(-(1.0 - LOOK_DOWN) * lean) * turns[1];
        ((1.0 - BACK_SHARE) * lean, self.stoop[2].max(0.0))
    }

    /// How far the body stoops to `load` (its lean, rad, and the hips' drop, m): along the way
    /// the server's hands assume the shoulders come down and on to what is held below them
    /// (`reach::stooped`), only as far as brings each hand's hold within [`EASY`] of its arm's
    /// reach, and at most all the way. (All the way at once, the body stooped to hold anything
    /// below its shoulders, a box at its elbows too.)
    fn stoop_needed(&self, load: &Load, up: DVec3, forward: DVec3) -> (f64, f64) {
        // Each hand's hold as last placed (at first, the point held) and the shoulders drawn
        // standing (the model's frame: x its left, y up, z the way it faces).
        let joints = bones::joints();
        let left = up.cross(forward).normalize_or(DVec3::X);
        let feet = self.centre - up * (character::heights().0 + self.drop);
        let reaches: [Option<(DVec3, DVec3, f64)>; 2] = std::array::from_fn(|side| {
            let (at, hold) = match self.grasps[side] {
                Some(g) => ((load.frame * g.frame).position, g.hold),
                None if side == 0 => (load.held(), Hold::Grip(0.0)),
                None => return None,
            };
            let s = joints[SHOULDER[side]].2;
            let shoulder = feet + left * s.x + up * s.y + forward * s.z;
            Some((
                at,
                shoulder,
                EASY * arm_reach(side, self.hands[side].rest(hold)),
            ))
        });
        let short = |moved: DVec3| {
            (reaches.iter().flatten())
                .map(|(at, shoulder, easy)| (*at - *shoulder - moved).length() - easy)
                .fold(f64::MIN, f64::max)
        };
        // Held within easy reach standing (as most things are carried): no stoop.
        if short(DVec3::ZERO) <= 0.0 {
            return (0.0, 0.0);
        }
        let standing = reach::shoulders(self.centre, up);
        let to = reach::stooped(standing, up, load.held()) - standing;
        let full = leaning(to.dot(forward).max(0.0));
        let squat = skeleton::shortest_leg() + SQUAT_SPARE + self.drop;
        let deepest = (hip_height() - squat).max(0.0);
        let down = (-to.dot(up) - lowered(full)).clamp(0.0, deepest);
        let short = |share: f64| {
            let (on, lowered) = ahead(share * full);
            short(forward * on - up * (lowered + share * down))
        };
        let share = if short(1.0) > 0.0 {
            1.0
        } else {
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..12 {
                let mid = 0.5 * (lo + hi);
                if short(mid) > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            hi
        };
        (share * full, share * down)
    }

    /// The arms of the drawn `bones` reaching where the hands hold, blended in and out (`dt`
    /// since the last frame, the body upright along `up`, facing `forward`, on `ground`).
    pub(super) fn reach_out(
        &mut self,
        mut bones: Vec<Frame64>,
        (up, forward): (DVec3, DVec3),
        dt: f64,
        ground: &dyn Ground,
    ) -> Vec<Frame64> {
        let step = if self.holding { dt } else { -dt } / REACHING;
        self.reaching = (self.reaching + step).clamp(0.0, 1.0);
        let Some(load) = self.held.filter(|_| self.reaching > 0.0) else {
            (self.held, self.grasps, self.holds) = (None, [None; 2], [Hold::Relaxed; 2]);
            return bones;
        };
        let joints = bones::joints();
        let torso = bones[1];
        let body = Reaching {
            from: SHOULDER.map(|j| torso.transform_point(joints[j].2)),
            up,
            forward,
        };
        let clear = |p: DVec3| {
            ground
                .under(p + up * 0.5, up)
                .is_none_or(|(g, _)| (p - g).dot(up) > CLEAR)
        };
        let reaches = load.reaches(&body, &self.hands, &clear);
        let w = self.reaching * self.reaching * (3.0 - 2.0 * self.reaching);
        let back = load.frame.inverse();
        for (side, reach) in reaches.into_iter().enumerate() {
            let Some(reach) = reach else {
                self.grasps[side] = None;
                self.holds[side] = Hold::Relaxed;
                continue;
            };
            // Slid to in the load's frame (it moves with the load at once).
            let want = back * reach.frame;
            let local = match self.grasps[side] {
                Some(was) if self.holding => slide(was.frame, want, dt),
                Some(was) => was.frame,
                None => want,
            };
            self.grasps[side] = Some(super::grasp::Reach {
                hold: reach.hold,
                frame: local,
            });
            self.holds[side] = if w > 0.5 { reach.hold } else { Hold::Relaxed };
            let rest = self.hands[side].rest(reach.hold);
            let (ik, _) = super::arms::arm(side, &torso, (rest, load.frame * local), up, forward);
            blend_arm(&mut bones, side, &ik, w);
        }
        bones
    }
}

/// How far the shoulders come forward leaning `lean` (rad) at the hips and the waist (m).
fn ahead(lean: f64) -> (f64, f64) {
    let ((on, down), _) = leant(lean);
    (on, down)
}

/// Leaning `lean` (rad), how far the shoulders come forward and down (m), and how fast they come
/// forward with the lean (m/rad). The trunk's lower part (hips to waist) turns by the hips'
/// share of the lean, its upper part (waist to shoulders) by all of it: each a turn about the
/// body's side, its parts' y up and z forward turned (y cos a − z sin a, y sin a + z cos a).
fn leant(lean: f64) -> ((f64, f64), f64) {
    let joints = bones::joints();
    let mid = |a: usize, b: usize| 0.5 * (joints[a].2 + joints[b].2);
    let (hips, waist, shoulders) = (
        mid(skeleton::HIP[0], skeleton::HIP[1]),
        joints[0].2,
        mid(SHOULDER[0], SHOULDER[1]),
    );
    let (lower, upper) = (waist - hips, shoulders - waist);
    let hips_share = 1.0 - BACK_SHARE;
    let (s1, c1) = creeer_math::det::sin_cos(hips_share * lean);
    let (s2, c2) = creeer_math::det::sin_cos(lean);
    let z = lower.y * s1 + lower.z * c1 + upper.y * s2 + upper.z * c2;
    let y = lower.y * c1 - lower.z * s1 + upper.y * c2 - upper.z * s2;
    let rate = hips_share * (lower.y * c1 - lower.z * s1) + upper.y * c2 - upper.z * s2;
    ((z - lower.z - upper.z, lower.y + upper.y - y), rate)
}

/// The lean that brings the shoulders `on` forward (rad), at most [`MOST_STOOP`]: by Newton's
/// method from upright (the shoulders come forward ever more slowly with the lean, so it
/// closes in from below).
fn leaning(on: f64) -> f64 {
    if ahead(MOST_STOOP).0 <= on {
        return MOST_STOOP;
    }
    let mut lean = 0.0;
    for _ in 0..8 {
        let ((at, _), rate) = leant(lean);
        lean = (lean + (on - at) / rate.max(1e-6)).min(MOST_STOOP);
    }
    lean
}

/// How far the shoulders come down leaning `lean` (m).
fn lowered(lean: f64) -> f64 {
    ahead(lean).1
}

/// How high the hips are above the ankles standing (m).
fn hip_height() -> f64 {
    let joints = bones::joints();
    joints[skeleton::HIP[0]].2.y - joints[skeleton::ANKLE[0]].2.y
}

/// `from` slid towards `to` by `dt`: at most [`SLIDE`] and [`SLIDE_TURN`] a second.
fn slide(from: Frame64, to: Frame64, dt: f64) -> Frame64 {
    if from == to {
        return to;
    }
    let off = to.position - from.position;
    let position = from.position + off.clamp_length_max(SLIDE * dt);
    let angle = from.rotation.angle_between(to.rotation);
    let share = if angle > 1e-9 {
        (SLIDE_TURN * dt / angle).min(1.0)
    } else {
        1.0
    };
    Frame64::new(position, from.rotation.slerp(to.rotation, share))
}

/// Arm `side` of `bones` turned `w` of the way at each joint (shoulder, elbow, wrist) to the
/// arm `ik`, then posed from the torso again (so it stays one arm while it blends).
fn blend_arm(bones: &mut [Frame64], side: usize, ik: &[Frame64; 3], w: f64) {
    let segments = [[3, 4, 5], [9, 10, 11]][side];
    if w >= 1.0 {
        for (k, &segment) in segments.iter().enumerate() {
            bones[segment] = ik[k];
        }
        return;
    }
    let joints = bones::joints();
    let at = [SHOULDER[side], ELBOW[side], ELBOW[side] + 1].map(|j| joints[j].2);
    let mut parent = bones[1];
    let (mut drawn_parent, mut ik_parent) = (bones[1], bones[1]);
    for (k, &segment) in segments.iter().enumerate() {
        let drawn = drawn_parent.rotation.inverse() * bones[segment].rotation;
        let wanted = ik_parent.rotation.inverse() * ik[k].rotation;
        let turn = drawn.slerp(wanted, w).normalize();
        (drawn_parent, ik_parent) = (bones[segment], ik[k]);
        bones[segment] = then(&parent, &about(at[k], turn));
        parent = bones[segment];
    }
}
