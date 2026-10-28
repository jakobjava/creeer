//! The drawn skeleton (G1.3g): the ragdoll's fifteen segments as bones, posed by turning each
//! joint about its place standing, and the legs placed by two-bone inverse kinematics. A bone
//! is the rigid motion of its segment from the model at rest to the world.

use creeer_math::Frame64;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_sim::character::bones;

/// The segments, by index in the ragdoll's order (`bones::SEGMENT_NAMES`): each side's forearm,
/// thigh, shank and foot, the right (−x) first.
#[cfg(test)]
pub const FOREARM: [usize; 2] = [4, 10];
pub const THIGH: [usize; 2] = [6, 12];
pub const SHANK: [usize; 2] = [7, 13];
pub const FOOT: [usize; 2] = [8, 14];
/// The joints, by index in `bones::joints()`.
pub const SHOULDER: [usize; 2] = [2, 8];
pub const ELBOW: [usize; 2] = [3, 9];
pub const HIP: [usize; 2] = [5, 11];
pub const KNEE: [usize; 2] = [6, 12];
pub const ANKLE: [usize; 2] = [7, 13];

/// A joint's turn of its segment about its place, as a rigid motion of the model at rest.
pub fn about(at: DVec3, turn: DQuat) -> Frame64 {
    Frame64::new(at - turn * at, turn)
}

/// `a` after `b`: x ↦ a(b(x)).
pub fn then(a: &Frame64, b: &Frame64) -> Frame64 {
    Frame64::new(
        a.position + a.rotation * b.position,
        a.rotation * b.rotation,
    )
}

/// Every bone, the pelvis's `root` and each joint turning its segment by `turns` (in its
/// parent's frame at rest), parents before children.
pub fn posed(root: Frame64, turns: &[DQuat; 14]) -> Vec<Frame64> {
    let mut out = vec![root; 15];
    for (j, (child, parent, at)) in bones::joints().into_iter().enumerate() {
        out[child] = then(&out[parent], &about(at, turns[j]));
    }
    out
}

/// The rotation taking the orthonormal frame (`u`, `s`) to (`u2`, `s2`): each its direction
/// and a side across it.
fn frame_turn(u: DVec3, s: DVec3, u2: DVec3, s2: DVec3) -> DQuat {
    let from = DMat3::from_cols(u, s, u.cross(s));
    let to = DMat3::from_cols(u2, s2, u2.cross(s2));
    DQuat::from_mat3(&(to * from.transpose())).normalize()
}

/// The part of `v` across unit `u`, made unit (`fallback` if none).
fn across(v: DVec3, u: DVec3, fallback: DVec3) -> DVec3 {
    (v - u * v.dot(u)).try_normalize().unwrap_or(fallback)
}

/// A two-bone limb at rest (G1.3g; solved exactly since G2.10): from its root joint by its
/// middle one to its end, bending at the middle about the model's x. Bent φ (a turn about +x),
/// its end lies √(K + 2D·cos(φ − φ₀)) from its root: the law of cosines with the bones' parts
/// along the hinge (which do not turn) set apart (as Tolani, Goswami and Badler 2000 solve an
/// arm's or leg's elbow analytically).
#[derive(Clone, Copy, Debug)]
pub struct Chain {
    k: f64,
    d: f64,
    /// The bend at which it reaches farthest (rad).
    pub straightest: f64,
}

impl Chain {
    /// The limb whose joints stand at `root`, `middle` and `end` at rest.
    pub fn new(root: DVec3, middle: DVec3, end: DVec3) -> Self {
        let (a, b) = (middle - root, end - middle);
        // a·R(φ)b = aₓbₓ + (a_y b_y + a_z b_z) cos φ + (a_z b_y − a_y b_z) sin φ.
        let (p, q) = (a.y * b.y + a.z * b.z, a.z * b.y - a.y * b.z);
        Self {
            k: a.length_squared() + b.length_squared() + 2.0 * a.x * b.x,
            d: creeer_math::det::hypot(p, q),
            straightest: creeer_math::det::atan2(q, p),
        }
    }

    /// How far its end is from its root bent `bend` (rad; m).
    pub fn span(&self, bend: f64) -> f64 {
        (self.k + 2.0 * self.d * creeer_math::det::cos(bend - self.straightest))
            .max(0.0)
            .sqrt()
    }

    /// How far either side of the straightest it bends to put its end `reach` from its root
    /// (rad): none when it reaches no further.
    pub fn off_straight(&self, reach: f64) -> f64 {
        let c = (reach * reach - self.k) / (2.0 * self.d.max(1e-12));
        creeer_math::det::acos(c.clamp(-1.0, 1.0))
    }
}

/// Leg `side` at rest as a chain: hip, knee, ankle.
fn leg_chain(side: usize) -> Chain {
    let joints = bones::joints();
    Chain::new(
        joints[HIP[side]].2,
        joints[KNEE[side]].2,
        joints[ANKLE[side]].2,
    )
}

/// The knee's most bend (rad).
const KNEE_MOST: f64 = 2.4;

/// The knee's flexion (rad, a turn about +x, which swings a hanging shank back) that puts the
/// ankle `reach` from the hip, for leg `side` at rest; the straightest when it reaches no
/// further. Standing, the knee is a little past straight: the span is longest a few degrees
/// bent, so a leg short of its reach bends less than the straightest (towards rest) if it reaches
/// as far as it does at rest, else more (to the range's end).
fn knee_for(side: usize, reach: f64) -> f64 {
    let chain = leg_chain(side);
    let straightest = chain.straightest;
    if reach >= chain.span(straightest) {
        return straightest;
    }
    let off = chain.off_straight(reach);
    if reach >= chain.span(0.0) {
        (straightest - off).max(0.0)
    } else {
        (straightest + off).min(KNEE_MOST)
    }
}

/// The shortest a leg reaches from hip to ankle (m), its knee bent its most: the longer leg's.
pub fn shortest_leg() -> f64 {
    (0..2)
        .map(|side| leg_chain(side).span(KNEE_MOST))
        .fold(0.0, f64::max)
}

/// Leg `side`'s knee turn from the model at rest at which it is straightest (rad): the model
/// stands with its knees a little past straight.
#[cfg(test)]
pub fn straight_knee(side: usize) -> f64 {
    knee_for(side, f64::INFINITY)
}

/// The knee's least bend in stance (rad): about 5°, as people's at its straightest (Perry).
const STANCE_KNEE: f64 = 5.0 * std::f64::consts::PI / 180.0;

/// The farthest a leg standing on its foot reaches from hip to ankle (m), the knee bent
/// [`STANCE_KNEE`] from straight: the shorter leg's. Straighter, the knee would swing fast for
/// a little reach (near straight its bend goes as the square root of the shortfall).
pub fn stance_leg() -> f64 {
    let joints = bones::joints();
    (0..2)
        .map(|side| {
            let (h, k, a) = (
                joints[HIP[side]].2,
                joints[KNEE[side]].2,
                joints[ANKLE[side]].2,
            );
            let bend = knee_for(side, f64::INFINITY) + STANCE_KNEE;
            (k + DQuat::from_rotation_x(bend) * (a - k) - h).length()
        })
        .fold(f64::INFINITY, f64::min)
}

/// The longest a leg reaches from hip to ankle (m), a hair short of straight: the shorter leg's.
pub fn longest_leg() -> f64 {
    let joints = bones::joints();
    (0..2)
        .map(|side| {
            let (h, k, a) = (
                joints[HIP[side]].2,
                joints[KNEE[side]].2,
                joints[ANKLE[side]].2,
            );
            (k + DQuat::from_rotation_x(knee_for(side, f64::INFINITY)) * (a - k) - h).length()
        })
        .fold(f64::INFINITY, f64::min)
        * 0.9999
}

/// A two-bone limb whose joints stand at `rest` (root, middle, end) placed: bent `bend` about
/// its middle's hinge (rest x), its root joint at `from` (world) and turned there so its end
/// points at `to`, the hinge along `hinge` made across that way (`fallback` the way if `to` is
/// at `from`). Returns its two bones and where its end is (beyond its reach, short of `to`).
pub fn limb(
    rest: [DVec3; 3],
    bend: f64,
    (from, to): (DVec3, DVec3),
    hinge: DVec3,
    fallback: DVec3,
) -> ([Frame64; 2], DVec3) {
    let [r, m, e] = rest;
    let flex = DQuat::from_rotation_x(bend);
    let bent = m + flex * (e - m);
    let rest_way = (bent - r).normalize();
    let way = (to - from).try_normalize().unwrap_or(fallback);
    let turn = frame_turn(
        rest_way,
        across(DVec3::X, rest_way, DVec3::Z),
        way,
        across(hinge, way, hinge),
    );
    let upper = Frame64::new(from - turn * r, turn);
    let lower = then(&upper, &about(m, flex));
    let end = lower.transform_point(e);
    ([upper, lower], end)
}

/// A leg placed by inverse kinematics: its hip joint at `hip` (world), the knee bent about its
/// hinge until the ankle can reach `ankle`, the leg then turned at the hip to reach it with
/// the knee's hinge across the way the person faces (`forward`, with `up`), and the foot flat
/// on ground of normal `ground`. The leg at rest gives the model at rest exactly. Returns the
/// thigh's, shank's and foot's bones, and how far the ankle fell short.
pub fn leg(
    side: usize,
    hip: DVec3,
    ankle: DVec3,
    forward: DVec3,
    up: DVec3,
    ground: DVec3,
) -> ([Frame64; 3], f64) {
    let joints = bones::joints();
    let rest = [
        joints[HIP[side]].2,
        joints[KNEE[side]].2,
        joints[ANKLE[side]].2,
    ];
    let a = rest[2];
    let bend = knee_for(side, (ankle - hip).length());
    let side_axis = up.cross(forward).try_normalize().unwrap_or(DVec3::X);
    let ([thigh, shank], reached) = limb(rest, bend, (hip, ankle), side_axis, -up);
    // The foot: its up the ground's normal, its forward the person's along the ground.
    let n = ground.try_normalize().unwrap_or(up);
    let ahead = across(forward, n, forward);
    let foot_turn = frame_turn(DVec3::Y, DVec3::Z, n, ahead);
    let foot = Frame64::new(reached - foot_turn * a, foot_turn);
    ([thigh, shank, foot], (ankle - reached).length())
}

#[cfg(test)]
#[path = "skeleton_tests.rs"]
mod tests;
