//! Procedural animation on the client (G1.3g, ADR-0026 decision 9): each character's bones
//! from its simulated body, its motion and the ground. The trunk is the simulated body (its
//! up, where it is), turned to the way it moves (the body itself, a box, never turns); the legs
//! walk or run at the cadence measured people use at its speed (`gait.rs`), their feet planted
//! where they land and placed by inverse kinematics on the ground found under them, the hips
//! lowered where a foot cannot reach (Johansen 2009); the arms swing against the legs. Fallen,
//! the bones are the ragdoll's segments exactly; standing up blends from there. Nothing is
//! sent: it is drawn from what the client already has.

pub mod feet;
pub mod gait;
pub mod skeleton;
mod upper;

use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::character::bones;
use feet::{Foot, Ground};
use std::collections::HashMap;

/// Below this speed a person stands (m/s).
const STILL: f64 = 0.15;
/// A foot this far from where it stands still steps there (m).
const SETTLE: f64 = 0.03;
/// The speed stepping into place goes at (m/s).
const SHUFFLE: f64 = 0.6;
/// How much faster than its gait a stride may hurry.
const HURRY: f64 = 3.0;
/// How far the hips may crouch before the stride hurries (m).
const CROUCH: f64 = 0.12;
/// How fast the motion's estimate follows the body (1/s), and the heading turns (rad/s).
const FOLLOW: f64 = 10.0;
const TURN: f64 = 8.0;
/// Further below the feet than this, the ground is not stood on (m): in the air, or on
/// something the client does not know as ground (a crate): the feet stand at the body's height.
const REACH: f64 = 0.3;
/// Water this deep at the body's centre and it treads water (m).
const SWIMS: f64 = 0.4;
/// Standing up from the ragdoll takes this long (s).
const RISE: f64 = 0.8;
/// The ankles' height above the soles (m): the model's at rest.
const ANKLE_HEIGHT: f64 = 0.111;

/// A character to animate: where its body is and whether it has fallen.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// Its body's centre and orientation.
    Standing(DVec3, DQuat),
    /// Its ragdoll's bones.
    Fallen(Vec<Frame64>),
}

#[derive(Clone, Debug)]
struct State {
    centre: DVec3,
    velocity: DVec3,
    heading: DVec3,
    /// Through the stride: 0 the right foot lands, 0.5 the left.
    phase: f64,
    feet: Option<[Foot; 2]>,
    time: f64,
    /// The last bones drawn, and standing up: the ragdoll's bones it rises from, and how long.
    last: Vec<Frame64>,
    rising: Option<(Vec<Frame64>, f64)>,
    fallen: bool,
    /// Stepping (not standing still).
    stepping: bool,
    /// How far the hips were lowered last (m).
    drop: f64,
    /// A tool's swing, through it (`Animator::swing`).
    swing: Option<f64>,
}

/// Every character's animation, by a key of the caller's (an entity).
#[derive(Debug, Default)]
pub struct Animator {
    states: HashMap<u32, State>,
}

impl Animator {
    /// Character `key`'s bones this frame, `dt` seconds after the last, its `body` as given.
    pub fn bones(&mut self, key: u32, body: &Body, dt: f64, ground: &dyn Ground) -> Vec<Frame64> {
        let state = self.states.entry(key).or_insert_with(|| State::new(body));
        state.time += dt;
        let bones = match body {
            Body::Fallen(bones) => {
                state.fallen = true;
                state.feet = None;
                bones.clone()
            }
            Body::Standing(centre, orientation) => {
                if state.fallen {
                    state.fallen = false;
                    state.rising = Some((state.last.clone(), 0.0));
                    state.centre = *centre;
                    state.velocity = DVec3::ZERO;
                }
                let standing = state.stand(*centre, *orientation, dt, ground);
                match &mut state.rising {
                    Some((from, t)) => {
                        *t += dt;
                        let x = (*t / RISE).min(1.0);
                        let blended = blend(from, &standing, x * x * (3.0 - 2.0 * x));
                        if x >= 1.0 {
                            state.rising = None;
                        }
                        blended
                    }
                    None => standing,
                }
            }
        };
        state.last.clone_from(&bones);
        bones
    }

    /// Character `key`'s tool swing, `progress` through it (0 raised, 1 struck; none: no
    /// swing): the hook G1.4's tools time to the server's strikes. The right arm follows it.
    // Called by G1.4's tools; until then by its test alone.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn swing(&mut self, key: u32, progress: Option<f64>) {
        if let Some(state) = self.states.get_mut(&key) {
            state.swing = progress;
        }
    }

    /// Forgets the characters not in `keep`.
    pub fn retain(&mut self, keep: &[u32]) {
        self.states.retain(|k, _| keep.contains(k));
    }
}

/// Each bone from `a` towards `b` by `x`.
fn blend(a: &[Frame64], b: &[Frame64], x: f64) -> Vec<Frame64> {
    a.iter()
        .zip(b)
        .map(|(a, b)| {
            Frame64::new(
                a.position.lerp(b.position, x),
                a.rotation.slerp(b.rotation, x),
            )
        })
        .collect()
}

impl State {
    fn new(body: &Body) -> Self {
        let (centre, orientation) = match body {
            Body::Standing(c, q) => (*c, *q),
            Body::Fallen(b) => (b[0].position, b[0].rotation),
        };
        Self {
            centre,
            velocity: DVec3::ZERO,
            heading: orientation * DVec3::Z,
            phase: 0.0,
            feet: None,
            time: 0.0,
            last: Vec::new(),
            rising: None,
            fallen: matches!(body, Body::Fallen(_)),
            stepping: false,
            drop: 0.0,
            swing: None,
        }
    }

    /// The bones standing, walking, running, in the air or in the water.
    fn stand(
        &mut self,
        centre: DVec3,
        orientation: DQuat,
        dt: f64,
        ground: &dyn Ground,
    ) -> Vec<Frame64> {
        let up = orientation * DVec3::Y;
        if dt > 0.0 {
            let seen = (centre - self.centre) / dt;
            self.velocity += (seen - self.velocity) * (FOLLOW * dt).min(1.0);
        }
        self.centre = centre;
        let along = self.velocity - up * self.velocity.dot(up);
        let speed = along.length();
        if speed > STILL {
            let want = along / speed;
            let angle = self.heading.angle_between(want);
            let step = (TURN * dt / angle.max(1e-9)).min(1.0);
            self.heading = self.heading.lerp(want, step).normalize_or(want);
        }
        let forward =
            (self.heading - up * self.heading.dot(up)).normalize_or(orientation * DVec3::Z);
        let turn = DQuat::from_rotation_arc(DVec3::Y, up);
        // About up by the signed angle from the model's forward to the heading (an arc between two
        // opposite directions could turn about any axis).
        let faces = turn * DVec3::Z;
        let angle = creeer_math::det::atan2(faces.cross(forward).dot(up), faces.dot(forward));
        let yaw = DQuat::from_axis_angle(up, angle);
        let rotation = (yaw * turn).normalize();
        let feet_at = centre - up * creeer_sim::character::heights().0;
        let mut root = Frame64::new(feet_at, rotation);
        let mut turns = [DQuat::IDENTITY; 14];
        // Where each foot stands still: under its ankle at rest.
        let joints = bones::joints();
        let spots = [7, 13].map(|j| {
            let a = joints[j].2;
            root.transform_point(DVec3::new(a.x, 0.0, a.z))
        });
        let ground_at = |p: DVec3| -> (DVec3, DVec3) {
            match ground.under(p + up * 0.5, up) {
                Some((g, n)) if (g - p).dot(up) > -REACH => (g, n),
                // Nothing to stand on within reach: as if on the level at the body's feet.
                _ => (p, up),
            }
        };
        if ground.depth(centre) > SWIMS {
            self.feet = None;
            upper::treading(&mut turns, self.time);
            return self.legs_free(root, turns, up, forward, |side, t| {
                let s = creeer_math::det::sin(
                    std::f64::consts::TAU * 0.8 * t + std::f64::consts::PI * side,
                );
                DVec3::new(0.0, 0.3, 0.15 * s)
            });
        }
        let airborne = spots.iter().all(|&s| {
            ground
                .under(s + up * 0.5, up)
                .is_none_or(|(g, _)| (g - s).dot(up) < -REACH)
        });
        if airborne && speed > STILL {
            self.feet = None;
            upper::airborne(&mut turns);
            return self.legs_free(root, turns, up, forward, |_, _| DVec3::new(0.0, 0.12, 0.08));
        }
        let rest = spots.map(&ground_at);
        let feet = self
            .feet
            .get_or_insert_with(|| rest.map(|(g, n)| Foot::planted(g, n)));
        let settled = feet
            .iter()
            .zip(&rest)
            .all(|(f, (g, _))| f.down && (f.plant - *g).length() < SETTLE);
        let (pace, aim) = if speed > STILL {
            (speed, along)
        } else if settled {
            self.stepping = false;
            (0.0, DVec3::ZERO)
        } else {
            (SHUFFLE, DVec3::ZERO)
        };
        let gait = gait::at(pace.max(SHUFFLE));
        if pace > 0.0 {
            if !self.stepping {
                // Setting off from standing: the right foot lifts at once (half a stride on, the
                // left is planted), or it would stay behind as the body went on.
                self.phase = gait.duty;
                self.stepping = true;
            }
            // A planted foot left further behind than a steady stride leaves it (setting off, or
            // speeding up) hurries the stride, as people take quick steps then.
            let stride_time = 2.0 / gait.cadence;
            let mut late = 0.0f64;
            for (side, (f, s)) in feet.iter().zip(&spots).enumerate() {
                #[allow(clippy::cast_precision_loss)] // 0 or 1
                let into = (self.phase - 0.5 * side as f64).rem_euclid(1.0);
                if f.down && into < gait.duty {
                    // Steady, a planted foot goes from half its stance's travel ahead of its spot
                    // to half behind.
                    let steady = pace * stride_time * (into - 0.5 * gait.duty);
                    let behind = (*s - f.plant).dot(aim) / pace.max(1e-9);
                    late = late.max(behind - steady - 0.03);
                }
            }
            // So does a stride that has the hips crouching (a foot left low behind on a slope).
            late = late.max(self.drop - CROUCH);
            let step = gait.step_length(pace.max(SHUFFLE));
            let hurry = (1.0 + 4.0 * late.max(0.0) / step).min(HURRY);
            self.phase = (self.phase + dt * hurry * gait.cadence / 2.0).fract();
            upper::moving(&mut turns, &gait, speed, self.phase);
        }
        if let Some(progress) = self.swing {
            upper::swing(&mut turns, progress);
        }
        // Each foot: its point of the stride, where it lands, where it is.
        let stride = 2.0 / gait.cadence;
        let mut targets = [(DVec3::ZERO, up); 2];
        for side in 0..2 {
            #[allow(clippy::cast_precision_loss)] // 0 or 1
            let along_stride = (self.phase - 0.5 * side as f64).rem_euclid(1.0);
            // To where it lands, half its time on the ground on: in the air, the time to landing;
            // just landed (the stride begun anew), the time since, counted back.
            let to_land = if along_stride >= gait.duty {
                (1.0 - along_stride) * stride
            } else {
                -along_stride * stride
            };
            let left = to_land + 0.5 * gait.duty * stride;
            let landing = feet::shortened(&ground_at, spots[side], aim * left, up);
            targets[side] = if pace > 0.0 {
                feet[side].step(along_stride, &gait, landing, up)
            } else {
                (feet[side].plant, feet[side].normal)
            };
        }
        // Each foot rolled over its stride: its ankle, and its sole's normal.
        let ankles: [(DVec3, DVec3); 2] = std::array::from_fn(|side| {
            #[allow(clippy::cast_precision_loss)] // 0 or 1
            let along_stride = (self.phase - 0.5 * side as f64).rem_euclid(1.0);
            if pace > 0.0 {
                feet::rolled(
                    side,
                    targets[side].0,
                    targets[side].1,
                    forward,
                    along_stride,
                    &gait,
                )
            } else {
                (
                    targets[side].0 + targets[side].1 * ANKLE_HEIGHT,
                    targets[side].1,
                )
            }
        });
        // The hips lowered as far as the farther foot needs (Johansen 2009).
        let longest = skeleton::longest_leg();
        let mut drop = 0.0f64;
        for side in 0..2 {
            let hip = root.transform_point(joints[skeleton::HIP[side]].2);
            let ankle = ankles[side].0;
            let to = ankle - hip;
            let down = -to.dot(up);
            let across = (to + up * down).length();
            let reach = (longest * longest - across * across).max(0.0).sqrt();
            drop = drop.max(down - reach);
        }
        self.drop = drop.max(0.0);
        root.position -= up * self.drop;
        let mut out = skeleton::posed(root, &turns);
        for side in 0..2 {
            let hip = root.transform_point(joints[skeleton::HIP[side]].2);
            let (ankle, n) = ankles[side];
            let (leg, _) = skeleton::leg(side, hip, ankle, forward, up, n);
            out[skeleton::THIGH[side]] = leg[0];
            out[skeleton::SHANK[side]] = leg[1];
            out[skeleton::FOOT[side]] = leg[2];
        }
        out
    }

    /// The bones with the legs free of the ground: each ankle at `place(side, time)` from its
    /// place at rest, in the body's frame.
    fn legs_free(
        &self,
        root: Frame64,
        turns: [DQuat; 14],
        up: DVec3,
        forward: DVec3,
        place: impl Fn(f64, f64) -> DVec3,
    ) -> Vec<Frame64> {
        let joints = bones::joints();
        let mut out = skeleton::posed(root, &turns);
        for side in 0..2 {
            let hip = root.transform_point(joints[skeleton::HIP[side]].2);
            #[allow(clippy::cast_precision_loss)] // 0 or 1
            let ankle = root
                .transform_point(joints[skeleton::ANKLE[side]].2 + place(side as f64, self.time));
            let (leg, _) = skeleton::leg(side, hip, ankle, forward, up, up);
            out[skeleton::THIGH[side]] = leg[0];
            out[skeleton::SHANK[side]] = leg[1];
            out[skeleton::FOOT[side]] = leg[2];
        }
        out
    }
}

#[cfg(test)]
#[path = "animate_tests.rs"]
mod tests;
