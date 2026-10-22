//! A character on its feet (G1.3g; G1.4a): standing, stepping, in the air or in the water,
//! each made as a pose in the frame of its feet (`pose.rs`), a change from one to another
//! smoothed by inertialization. Stopping ends in the step that brings the feet beside each
//! other (people stop within about a step: Ridge et al. 2013), and no steps are taken in
//! place after it unless a foot stands well off where it would.

use super::feet::{self, Foot, Ground, Through};
use super::pose::{Mode, Pose};
use super::{State, gait, skeleton, upper};
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::character::{JUMP_SPEED, bones};

/// Below this speed a person stands (m/s).
const STILL: f64 = 0.15;
/// A foot this far from where it would stand still steps there (m): about half a boot's
/// length, an offset that shows; nearer, the person stands as the feet came to rest.
const SETTLE: f64 = 0.12;
/// The speed stepping into place goes at (m/s).
const SHUFFLE: f64 = 0.6;
/// How much faster than its gait a stride may hurry.
const HURRY: f64 = 3.0;
/// How far the hips may crouch before the stride hurries (m).
const CROUCH: f64 = 0.12;
/// How fast the motion's estimates follow the body (1/s), and the heading turns (rad/s).
const FOLLOW: f64 = 10.0;
const TURN: f64 = 8.0;
/// How fast the hips come back up when no foot holds them down (1/s): critically damped, as
/// fast as lets the knee straighten through a walking stance to about 5° as people's does
/// (Perry: the stance knee 15–20° at its most bent, about 5° at mid-stance).
const HIPS: f64 = 40.0;
/// How long the arms take to change their carriage between walking and running (s): within a
/// step, not within a frame.
const CARRIAGE: f64 = 0.15;
/// Further below the feet than this, the ground is not stood on (m): in the air, or on
/// something the client does not know as ground (a crate): the feet stand at the body's height.
const REACH: f64 = 0.3;
/// Water this deep at the body's centre and it treads water (m).
const SWIMS: f64 = 0.4;
/// The ankles' height above the soles (m): the model's at rest.
const ANKLE_HEIGHT: f64 = 0.111;
/// Rising faster than this it has jumped (m/s): half a jump's speed, as the server allows a
/// jump only below it.
const TAKE_OFF: f64 = 0.5 * JUMP_SPEED;
/// The feet's level this far above the ground has left it (m).
const LIFT_OFF: f64 = 0.03;
/// Gathering speed downward faster than this, it falls freely (m/s²): half of gravity.
const FREE_FALL: f64 = 0.5 * 9.806_65;
/// The ankles drawn up and forward at the top of a flight (m, in the body's frame).
const TUCK: DVec3 = DVec3::new(0.0, 0.12, 0.08);

impl State {
    /// The bones standing, walking, running, in the air or in the water.
    pub(super) fn stand(
        &mut self,
        centre: DVec3,
        orientation: DQuat,
        dt: f64,
        ground: &dyn Ground,
    ) -> Vec<Frame64> {
        let up = orientation * DVec3::Y;
        let before = self.velocity;
        let moved = centre - self.centre;
        if dt > 0.0 {
            let seen = (centre - self.centre) / dt;
            self.velocity += (seen - self.velocity) * (FOLLOW * dt).min(1.0);
            let climb = seen.dot(up);
            let sinking = (self.climb - climb) / dt;
            self.sinking += (sinking - self.sinking) * (FOLLOW * dt).min(1.0);
            self.climb = climb;
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
        if dt > 0.0 {
            let ahead = (self.velocity - before).dot(forward) / dt;
            self.ahead += (ahead - self.ahead) * (FOLLOW * dt).min(1.0);
        }
        let turn = DQuat::from_rotation_arc(DVec3::Y, up);
        // About up by the signed angle from the model's forward to the heading (an arc between two
        // opposite directions could turn about any axis).
        let faces = turn * DVec3::Z;
        let angle = creeer_math::det::atan2(faces.cross(forward).dot(up), faces.dot(forward));
        let yaw = DQuat::from_axis_angle(up, angle);
        let rotation = (yaw * turn).normalize();
        let base = Frame64::new(centre - up * creeer_sim::character::heights().0, rotation);
        let (mode, root, turns, ankles) = self.pose(base, up, forward, speed, along, dt, ground);
        let transition = mode != self.mode;
        self.mode = mode;
        let local = Pose::local(&base, root, turns, ankles);
        // Leaving the ground, the feet go with the body the moment it goes (a jump is a push the
        // simulation gives at once).
        let carry = if matches!(mode, Mode::Airborne | Mode::Swimming) {
            moved
        } else {
            DVec3::ZERO
        };
        let drawn = self.smoother.draw(local, transition, dt, carry);
        drawn.bones(&base, forward, up)
    }

    /// What the body does now, and its pose in the world: the pelvis, the joints' turns and the
    /// ankles with their soles' normals.
    #[allow(clippy::too_many_arguments)]
    fn pose(
        &mut self,
        base: Frame64,
        up: DVec3,
        forward: DVec3,
        speed: f64,
        along: DVec3,
        dt: f64,
        ground: &dyn Ground,
    ) -> (Mode, Frame64, [DQuat; 14], [(DVec3, DVec3); 2]) {
        let mut root = base;
        let mut turns = [DQuat::IDENTITY; 14];
        let joints = bones::joints();
        // Where each foot stands still: under its ankle at rest.
        let spots = [7, 13].map(|j| {
            let a = joints[j].2;
            root.transform_point(DVec3::new(a.x, 0.0, a.z))
        });
        // The ground within reach under a point; with nothing to stand on within reach, the feet
        // stand as if on the level at the body's feet.
        let ground_at = |p: DVec3| {
            ground
                .under(p + up * 0.5, up)
                .filter(|(g, _)| (*g - p).dot(up) > -REACH)
        };
        let level = |p: DVec3| ground_at(p).unwrap_or((p, up));
        if ground.depth(self.centre) > SWIMS {
            self.feet = None;
            upper::treading(&mut turns, self.time);
            let ankles = free(
                root,
                |side, t| {
                    let s = creeer_math::det::sin(
                        std::f64::consts::TAU * 0.8 * t + std::f64::consts::PI * side,
                    );
                    DVec3::new(0.0, 0.3, 0.15 * s)
                },
                self.time,
                up,
            );
            return (Mode::Swimming, root, turns, ankles);
        }
        let beyond = spots.iter().all(|&s| {
            ground
                .under(s + up * 0.5, up)
                .is_none_or(|(g, _)| (g - s).dot(up) < -REACH)
        });
        // How far the feet's level is above the ground under the body.
        let clear = ground
            .under(base.position + up * 0.5, up)
            .map_or(f64::INFINITY, |(g, _)| (base.position - g).dot(up));
        // In the air: moving with nothing within reach underfoot; or pushed off the ground
        // rising as a jump leaves it, the feet leaving it with the body (held down they would
        // pin the hips); and from either, as long as it falls freely with ground still below.
        let flying = (beyond && speed > STILL)
            || (self.climb > TAKE_OFF && clear > LIFT_OFF)
            || (self.mode == Mode::Airborne && clear > LIFT_OFF && self.sinking > FREE_FALL);
        if flying {
            self.feet = None;
            upper::airborne(&mut turns);
            // The legs drawn up at the top of a flight, straight as they leave the ground and as
            // they meet it again.
            let rise = (self.climb / JUMP_SPEED).clamp(-1.0, 1.0);
            let tuck = 1.0 - rise * rise;
            let ankles = free(root, |_, _| TUCK * tuck, self.time, up);
            return (Mode::Airborne, root, turns, ankles);
        }
        let rest = spots.map(level);
        let fresh = self.feet.is_none();
        if fresh {
            // Down from the air or out of the water on both feet: a stride begins anew, as from
            // standing.
            (self.stepping, self.stopping) = (false, false);
        }
        let feet = self
            .feet
            .get_or_insert_with(|| rest.map(|(g, n)| Foot::planted(g, n)));
        let settled = feet
            .iter()
            .zip(&rest)
            .all(|(f, (g, _))| f.down && (f.plant - *g).length() < SETTLE);
        let moving = speed > STILL;
        if moving {
            self.stopping = false;
            self.pace = speed;
        } else if self.stepping && !self.stopping {
            // Stopping: the stride goes on, slowing, until the swinging foot lands beside the other.
            self.stopping = true;
        }
        let (pace, aim) = if moving {
            (speed, along)
        } else if self.stopping {
            (self.pace.max(SHUFFLE), DVec3::ZERO)
        } else if settled {
            self.stepping = false;
            (0.0, DVec3::ZERO)
        } else {
            (SHUFFLE, DVec3::ZERO)
        };
        let gait = gait::at(pace.max(SHUFFLE));
        self.carriage += (gait.running - self.carriage) * (dt / CARRIAGE).min(1.0);
        let lean = upper::lean(self.carriage, self.ahead);
        if pace > 0.0 {
            if !self.stepping {
                // Setting off from standing: the right foot lifts at once (half a stride on, the
                // left is planted), or it would stay behind as the body went on.
                self.phase = gait.duty;
                self.stepping = true;
                for f in feet.iter_mut() {
                    f.duty = gait.duty;
                }
                // That first foot lifts flat: at rest, it had not rolled onto its toe.
                feet[0].from_rest = true;
            }
            // A planted foot left further behind than a steady stride leaves it (setting off, or
            // speeding up) hurries the stride, as people take quick steps then.
            let stride_time = 2.0 / gait.cadence;
            let mut late = 0.0f64;
            for (side, (f, s)) in feet.iter().zip(&spots).enumerate() {
                #[allow(clippy::cast_precision_loss)] // 0 or 1
                let into = (self.phase - 0.5 * side as f64).rem_euclid(1.0);
                if f.down && into < f.duty && aim != DVec3::ZERO {
                    // Steady, a planted foot goes from half its stance's travel ahead of its spot
                    // to half behind.
                    let steady = pace * stride_time * (into - 0.5 * f.duty);
                    let behind = (*s - f.plant).dot(aim) / pace.max(1e-9);
                    late = late.max(behind - steady - 0.03);
                }
            }
            // So does a stride that has the hips crouching (a foot left low behind on a slope).
            late = late.max(self.drop - CROUCH);
            let step = gait.step_length(pace.max(SHUFFLE));
            let hurry = (1.0 + 4.0 * late.max(0.0) / step).min(HURRY);
            self.phase = (self.phase + dt * hurry * gait.cadence / 2.0).fract();
            upper::moving(&mut turns, &gait, speed, self.carriage, self.phase, lean);
        } else {
            upper::standing(&mut turns, lean);
        }
        if let Some(progress) = self.swing {
            upper::swing(&mut turns, progress);
        }
        // Each foot: its point of the stride, where it lands, where it is.
        let stride = 2.0 / gait.cadence;
        let mut targets = [(DVec3::ZERO, up); 2];
        let mut throughs = [Through::Stance(0.5); 2];
        let mut lands_in = [0.0; 2];
        for side in 0..2 {
            #[allow(clippy::cast_precision_loss)] // 0 or 1
            let along_stride = (self.phase - 0.5 * side as f64).rem_euclid(1.0);
            // To where it lands, half its time on the ground on: in the air, the time to landing;
            // just landed (the stride begun anew), the time since, counted back.
            let f = &feet[side];
            let swinging = along_stride >= if f.down { f.duty } else { f.lift_at };
            let to_land = if swinging {
                (1.0 - along_stride) * stride
            } else {
                -along_stride * stride
            };
            let left = to_land + 0.5 * gait.duty * stride;
            let landing = feet::shortened(&ground_at, spots[side], aim * left, up);
            lands_in[side] = to_land.max(0.0);
            if pace > 0.0 {
                (targets[side], throughs[side]) = feet[side].step(along_stride, &gait, landing, up);
            } else {
                targets[side] = (feet[side].plant, feet[side].normal);
            }
        }
        // Stopped once both feet stand: the stride that brought them together is done.
        if self.stopping && feet.iter().all(|f| f.down) {
            self.stopping = false;
            self.stepping = false;
        }
        // Each foot rolled over its stride: its ankle, and its sole's normal.
        let ankles: [(DVec3, DVec3); 2] = std::array::from_fn(|side| {
            if pace > 0.0 && self.stepping {
                feet::rolled(
                    side,
                    targets[side].0,
                    targets[side].1,
                    forward,
                    throughs[side],
                    feet[side].from_rest,
                )
            } else {
                (
                    targets[side].0 + targets[side].1 * ANKLE_HEIGHT,
                    targets[side].1,
                )
            }
        });
        // The hips lowered as far as the farther foot needs (Johansen 2009). A foot in the air
        // need not be reached until it lands: through its swing, its need goes over from where it
        // is to where it will land, from the hips as they will be then, so the hips go up and down
        // on a smooth path (Saunders, Inman and Eberhart 1953), not all at once as a foot leaves
        // the ground or meets it.
        let longest = skeleton::longest_leg();
        let need = |side: usize, ankle: DVec3, later: f64| {
            let hip = root.transform_point(joints[skeleton::HIP[side]].2)
                + along * later
                + forward * (0.5 * self.ahead * later * later);
            let to = ankle - hip;
            let down = -to.dot(up);
            let across = (to + up * down).length();
            down - (longest * longest - across * across).max(0.0).sqrt()
        };
        let mut drop = 0.0f64;
        for (side, (ankle, _)) in ankles.iter().enumerate() {
            let now = need(side, *ankle, 0.0);
            if let (true, Through::Swing(s)) = (pace > 0.0 && self.stepping, throughs[side]) {
                let (g, n) = feet[side].aimed;
                let (lands, _) = feet::rolled(side, g, n, forward, Through::Stance(0.0), false);
                let then = need(side, lands, lands_in[side]);
                drop = drop.max(now + (then - now) * s * s * (3.0 - 2.0 * s));
            } else {
                drop = drop.max(now);
            }
        }
        // Lowered at once as far as a foot needs (it must be reached); coming back up, the hips
        // follow as a mass does, critically damped, so their speed never jumps.
        let needed = drop.max(0.0);
        let was = self.drop;
        let pull = HIPS * HIPS * (needed - self.drop) - 2.0 * HIPS * self.lowering;
        self.lowering += pull * dt;
        self.drop += self.lowering * dt;
        if fresh {
            // A stance begun: the hips where it needs them, still (a change the pose smooths).
            (self.drop, self.lowering) = (needed, 0.0);
        } else if self.drop < needed {
            self.drop = needed;
            self.lowering = if dt > 0.0 { (needed - was) / dt } else { 0.0 };
        }
        root.position -= up * self.drop;
        // The trunk leans about the hips, so the legs keep their reach.
        let hips = 0.5 * (joints[skeleton::HIP[0]].2 + joints[skeleton::HIP[1]].2);
        let at = root.transform_point(hips);
        let tilt = DQuat::from_axis_angle(up.cross(forward).normalize_or(DVec3::X), lean);
        root.rotation = (tilt * root.rotation).normalize();
        root.position = at - root.rotation * hips;
        let mode = if self.stepping {
            Mode::Stepping
        } else {
            Mode::Standing
        };
        (mode, root, turns, ankles)
    }
}

/// The ankles with the legs free of the ground: each at `place(side, time)` from its place at
/// rest, in the body's frame `root`, the soles turned to `up`.
fn free(
    root: Frame64,
    place: impl Fn(f64, f64) -> DVec3,
    time: f64,
    up: DVec3,
) -> [(DVec3, DVec3); 2] {
    let joints = bones::joints();
    std::array::from_fn(|side| {
        #[allow(clippy::cast_precision_loss)] // 0 or 1
        let ankle =
            root.transform_point(joints[skeleton::ANKLE[side]].2 + place(side as f64, time));
        (ankle, up)
    })
}
