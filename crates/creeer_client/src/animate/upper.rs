//! The body above the legs (G1.3g; measured in G1.4a): the arms swing against the legs as
//! measured, the trunk leans into its speed and its acceleration, the head keeps its eyes
//! level. Treading water, the arms scull. Turns are of each joint, in its parent's frame at
//! rest (+x turns a hanging limb back, a standing one forward).
//!
//! - **Walking arms** (Murray, Sepic and Barnard 1967, 30 men at 1.54 and 2.14 m/s): the
//!   shoulder furthest back (24°, 31° fast) at its own side's heel-strike and furthest forward
//!   (8°) at the other's, so the arm swings mostly behind the body; the elbow never straight,
//!   bending from 17° to 47° (15° to 55° fast) with the shoulder, about 0.05 s after it.
//! - **Running arms**: bent near 90° at the elbow (Yegian and Tucker 2019, "straight arm
//!   walking, bent arm running"), the swing of fast walking.
//! - **The lean**: 7.3° forward running at 3.4 m/s (Teng and Powers 2014, recreational runners'
//!   own), and accelerating or braking at a, the line from the feet to the body's centre tilted
//!   by atan(a/g), as it must be for the push on the ground to pass through the centre.

use super::gait::Gait;
use super::skeleton::{ELBOW, SHOULDER};
use creeer_math::det;
use creeer_math::glam::DQuat;
use creeer_sim::character::bones;
use std::f64::consts::TAU;

/// The joints `back` and `neck` (`bones::joints()`).
const BACK: usize = 0;
const NECK: usize = 1;

/// An arm's swing walking at a speed (degrees): the shoulder's furthest back and forward, and
/// the elbow's least and most bent.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Swing {
    back: f64,
    forward: f64,
    elbow: (f64, f64),
}

/// Murray, Sepic and Barnard (1967): free and fast walking.
const FREE: (f64, Swing) = (
    1.54,
    Swing {
        back: 24.0,
        forward: 8.0,
        elbow: (17.0, 47.0),
    },
);
const FAST: (f64, Swing) = (
    2.14,
    Swing {
        back: 31.0,
        forward: 8.0,
        elbow: (15.0, 55.0),
    },
);
/// The elbow follows the shoulder by this (s).
const ELBOW_LAG: f64 = 0.05;
/// Running: the elbow's bend about 90°, through fast walking's range either side (degrees).
const RUN_ELBOW: f64 = 90.0;
/// The trunk's lean running (Teng and Powers 2014, at 3.4 m/s) (rad).
const RUN_LEAN: f64 = 7.3 * std::f64::consts::PI / 180.0;
/// Standard gravity (m/s²).
const G: f64 = 9.806_65;
/// The most the trunk leans either way (rad): a sprinter's start.
const MOST_LEAN: f64 = 0.6;

/// The elbow's bend in the model at rest (degrees): the forearm hangs about 14.5° forward of
/// the upper arm. Murray's and the running angles are the joint's own, so the turns drawn from
/// the model are less by it.
pub fn rest_elbow() -> f64 {
    let joints = bones::joints();
    let (shoulder, elbow, wrist) = (
        joints[SHOULDER[0]].2,
        joints[ELBOW[0]].2,
        joints[ELBOW[0] + 1].2,
    );
    // Forward of hanging straight down, in the elbow's plane (about x).
    let forward = |v: creeer_math::glam::DVec3| det::atan2(v.z, -v.y);
    (forward(wrist - elbow) - forward(elbow - shoulder)).to_degrees()
}

/// Walking at `speed`: Murray's two speeds, and between standing (the arms hanging, the elbows
/// at their least bend) and the free speed, a share of it.
fn walking(speed: f64) -> Swing {
    let (v0, a) = FREE;
    let (v1, b) = FAST;
    if speed <= v0 {
        let s = (speed / v0).max(0.0);
        return Swing {
            back: a.back * s,
            forward: a.forward * s,
            elbow: (a.elbow.0, a.elbow.0 + (a.elbow.1 - a.elbow.0) * s),
        };
    }
    let t = ((speed - v0) / (v1 - v0)).min(1.0);
    let mix = |x: f64, y: f64| x + (y - x) * t;
    Swing {
        back: mix(a.back, b.back),
        forward: mix(a.forward, b.forward),
        elbow: (mix(a.elbow.0, b.elbow.0), mix(a.elbow.1, b.elbow.1)),
    }
}

/// The swing at `speed`, `running` (0 to 1) of the way to a run.
fn swing_at(speed: f64, running: f64) -> Swing {
    let walk = walking(speed);
    let half = 0.5 * (FAST.1.elbow.1 - FAST.1.elbow.0);
    let run = Swing {
        back: FAST.1.back,
        forward: FAST.1.forward,
        elbow: (RUN_ELBOW - half, RUN_ELBOW + half),
    };
    let mix = |x: f64, y: f64| x + (y - x) * running;
    Swing {
        back: mix(walk.back, run.back),
        forward: mix(walk.forward, run.forward),
        elbow: (
            mix(walk.elbow.0, run.elbow.0),
            mix(walk.elbow.1, run.elbow.1),
        ),
    }
}

/// The trunk's lean (rad, forward positive) at `running` (0 to 1) of a run, accelerating at
/// `ahead` (m/s², along the way it faces).
pub fn lean(running: f64, ahead: f64) -> f64 {
    (RUN_LEAN * running + det::atan(ahead / G)).clamp(-MOST_LEAN, MOST_LEAN)
}

/// The upper body's joint turns at `speed`, `running` (0 to 1, as the arms carry themselves),
/// `phase` through the stride (0: the right foot lands) of `gait`, the trunk leaning `lean`
/// (the head kept level).
pub fn moving(
    turns: &mut [DQuat; 14],
    gait: &Gait,
    speed: f64,
    running: f64,
    phase: f64,
    lean: f64,
) {
    let s = swing_at(speed, running);
    let lag = ELBOW_LAG * gait.cadence / 2.0;
    let (mid, amp) = (0.5 * (s.forward - s.back), 0.5 * (s.forward + s.back));
    let (mid_e, amp_e) = (0.5 * (s.elbow.0 + s.elbow.1), 0.5 * (s.elbow.1 - s.elbow.0));
    for side in 0..2 {
        // Since its own side's heel-strike: the right foot lands at 0, the left at a half.
        #[allow(clippy::cast_precision_loss)] // 0 or 1
        let since = phase - 0.5 * side as f64;
        let shoulder = mid - amp * det::cos(TAU * since);
        let elbow = mid_e - amp_e * det::cos(TAU * (since - lag));
        turns[SHOULDER[side]] = DQuat::from_rotation_x(-shoulder.to_radians());
        turns[ELBOW[side]] = DQuat::from_rotation_x(-(elbow - rest_elbow()).to_radians());
    }
    turns[BACK] = DQuat::IDENTITY;
    turns[NECK] = DQuat::from_rotation_x(-lean);
}

/// Standing: the arms hanging, the elbows a little bent (walking's least bend), leaning `lean`.
pub fn standing(turns: &mut [DQuat; 14], lean: f64) {
    for side in 0..2 {
        turns[SHOULDER[side]] = DQuat::IDENTITY;
        turns[ELBOW[side]] = DQuat::from_rotation_x(-(FREE.1.elbow.0 - rest_elbow()).to_radians());
    }
    turns[NECK] = DQuat::from_rotation_x(-lean);
}

/// Treading water, `time` seconds in: the arms out and sculling, forearms forward.
pub fn treading(turns: &mut [DQuat; 14], time: f64) {
    let scull = det::sin(TAU * 0.8 * time);
    for side in 0..2 {
        // Out from the body (about z, outward on each side), sweeping forward and back.
        let out = if side == 0 { -1.1 } else { 1.1 };
        turns[SHOULDER[side]] = DQuat::from_rotation_z(out) * DQuat::from_rotation_y(0.4 * scull);
        turns[ELBOW[side]] = DQuat::from_rotation_x(-0.6);
    }
    turns[BACK] = DQuat::IDENTITY;
    turns[NECK] = DQuat::IDENTITY;
}

/// In the air: the arms a little out for balance.
pub fn airborne(turns: &mut [DQuat; 14]) {
    for side in 0..2 {
        let out = if side == 0 { -0.35 } else { 0.35 };
        turns[SHOULDER[side]] = DQuat::from_rotation_z(out);
        turns[ELBOW[side]] = DQuat::from_rotation_x(-0.5);
    }
}

/// The right arm swinging a tool, `progress` through it: raised overhead at 0, struck down in
/// front at 1 (the shoulder from 150° forward to 30°, the elbow straightening).
pub fn swing(turns: &mut [DQuat; 14], progress: f64) {
    let x = progress.clamp(0.0, 1.0);
    turns[SHOULDER[0]] = DQuat::from_rotation_x(-(2.6 - 2.1 * x));
    turns[ELBOW[0]] = DQuat::from_rotation_x(-(1.2 - 1.0 * x));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_arms_swing_as_measured() {
        let s = walking(1.54);
        assert_eq!((s.back, s.forward, s.elbow), (24.0, 8.0, (17.0, 47.0)));
        // Standing: hanging, the elbows at their least bend.
        assert_eq!(
            walking(0.0),
            Swing {
                back: 0.0,
                forward: 0.0,
                elbow: (17.0, 17.0)
            }
        );
        // Running: the elbow about a right angle.
        let run = swing_at(3.5, 1.0);
        assert!((0.5 * (run.elbow.0 + run.elbow.1) - 90.0).abs() < 1e-9);
        // The model's own elbow at rest, which the drawn turns leave out: 14.5°, the forearm
        // 16.1° forward of hanging, the upper arm 1.7°.
        assert!((rest_elbow() - 14.47).abs() < 0.01, "{}", rest_elbow());
    }

    #[test]
    fn the_trunk_leans_into_a_run_and_its_acceleration() {
        assert!((lean(1.0, 0.0) - RUN_LEAN).abs() < 1e-12);
        assert!(lean(0.0, 0.0).abs() < 1e-12);
        // Braking leans back; a hard start leans far forward, as far as the cap.
        assert!(lean(0.0, -3.0) < -0.25);
        assert!((lean(1.0, 40.0) - MOST_LEAN).abs() < 1e-12);
    }
}
