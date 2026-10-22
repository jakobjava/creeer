//! The body above the legs (G1.3g): the arms swing against the legs, the elbows bend more as
//! the gait runs, and the trunk leans into its speed; the head keeps its eyes level. Treading
//! water, the arms scull. Turns are of each joint, in its parent's frame at rest (+x turns a
//! hanging limb back, a standing one forward).

use super::gait::Gait;
use super::skeleton::{ELBOW, SHOULDER};
use creeer_math::det;
use creeer_math::glam::DQuat;
use std::f64::consts::TAU;

/// The joints `back` and `neck` (`bones::joints()`).
const BACK: usize = 0;
const NECK: usize = 1;
/// The shoulders' swing each way, walking at 1.4 m/s and running (rad): about ±16° and ±35°.
const SWING: (f64, f64) = (0.28, 0.6);
/// The elbows' bend, walking and running (rad): about 20° and 85°.
const ELBOWS: (f64, f64) = (0.35, 1.5);
/// The trunk's lean into the speed, walking and running (rad).
const LEAN: (f64, f64) = (0.04, 0.16);
/// The speed the walking swing is measured at (m/s).
const WALK: f64 = 1.4;

/// The upper body's joint turns walking or running at `speed`, `phase` through the stride
/// (0: the right foot lands).
pub fn moving(turns: &mut [DQuat; 14], gait: &Gait, speed: f64, phase: f64) {
    let r = gait.running;
    // Still, the arms hang; they swing more as the person walks faster.
    let amount = (speed / WALK).min(1.0);
    let swing = (SWING.0 + (SWING.1 - SWING.0) * r) * amount;
    let elbows = (ELBOWS.0 + (ELBOWS.1 - ELBOWS.0) * r) * amount;
    let lean = (LEAN.0 + (LEAN.1 - LEAN.0) * r) * amount;
    turns[BACK] = DQuat::from_rotation_x(lean);
    turns[NECK] = DQuat::from_rotation_x(-0.7 * lean);
    for side in 0..2 {
        // Each arm forward as the other side's leg lands: the right arm at the left foot's
        // landing, half a stride on.
        #[allow(clippy::cast_precision_loss)] // 0 or 1
        let forward = det::cos(TAU * (phase - 0.5 * (1.0 - side as f64)));
        turns[SHOULDER[side]] = DQuat::from_rotation_x(-swing * forward);
        turns[ELBOW[side]] = DQuat::from_rotation_x(-elbows - 0.3 * swing * forward.max(0.0));
    }
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
