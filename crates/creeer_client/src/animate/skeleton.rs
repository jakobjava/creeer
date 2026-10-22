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
fn about(at: DVec3, turn: DQuat) -> Frame64 {
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

/// The knee's flexion (rad, a turn about +x, which swings a hanging shank back) that puts the
/// ankle `reach` from the hip, for leg `side` at rest; the straightest when it reaches no
/// further.
fn knee_for(side: usize, reach: f64) -> f64 {
    let joints = bones::joints();
    let (h, k, a) = (
        joints[HIP[side]].2,
        joints[KNEE[side]].2,
        joints[ANKLE[side]].2,
    );
    let span = |angle: f64| (k + DQuat::from_rotation_x(angle) * (a - k) - h).length();
    // Standing, the knee is a little past straight: the span is longest a few degrees bent.
    let (mut lo, mut hi) = (0.0f64, 0.6f64);
    for _ in 0..60 {
        let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if span(a) < span(b) {
            lo = a;
        } else {
            hi = b;
        }
    }
    let straightest = 0.5 * (lo + hi);
    if reach >= span(straightest) {
        return straightest;
    }
    // Short of it: bent less than the straightest (from rest), or more (to the range's end).
    let (mut lo, mut hi, growing) = if reach >= span(0.0) {
        (0.0, straightest, true)
    } else {
        (straightest, 2.4, false)
    };
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if (span(mid) < reach) == growing {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
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
    let (h, k, a) = (
        joints[HIP[side]].2,
        joints[KNEE[side]].2,
        joints[ANKLE[side]].2,
    );
    let reach = (ankle - hip).length();
    let flex = DQuat::from_rotation_x(knee_for(side, reach));
    let bent = k + flex * (a - k);
    let rest = (bent - h).normalize();
    let to = (ankle - hip).try_normalize().unwrap_or(-up);
    let side_axis = up.cross(forward).try_normalize().unwrap_or(DVec3::X);
    let turn = frame_turn(
        rest,
        across(DVec3::X, rest, DVec3::Z),
        to,
        across(side_axis, to, side_axis),
    );
    let thigh = Frame64::new(hip - turn * h, turn);
    let shank = then(&thigh, &about(k, flex));
    let reached = shank.transform_point(a);
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
