//! A character on its feet (G1.3g; G1.4a): standing, stepping, in the air or in the water,
//! each made as a pose in the frame of its feet (`pose.rs`), a change from one to another
//! smoothed by inertialization. Stopping ends in the step that brings the feet beside each
//! other (people stop within about a step: Ridge et al. 2013), and no steps are taken in
//! place after it unless a foot stands well off where it would.

use super::feet::{self, Foot, Ground, Through};
use super::pose::free;
use super::pose::{Mode, Pose};
use super::{State, gait, hips, skeleton, upper};
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
/// Unmoved this long, the body is still (s): nine ticks. The player's avatar moves each tick
/// (predicted a tick at a time), others each frame (interpolated).
const STALE: f64 = 0.15;
/// The fastest the stopping steps go (m/s): a free walk.
const STOP_PACE: f64 = 1.4;
/// Slowing faster than this, it is braking to a stop, not walking more slowly (m/s²).
const BRAKING: f64 = 0.5;
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
        // The body's motion measured over the time since it was last seen to move: drawn faster
        // than the simulation ticks, the same place comes several frames running and then a
        // whole tick's move at once (as frame-to-frame it would look still, then fast). Unmoved
        // for longer than a tick could take, it is still.
        self.unseen += dt;
        let span = if moved != DVec3::ZERO || self.unseen > STALE {
            std::mem::take(&mut self.unseen)
        } else {
            0.0
        };
        if span > 0.0 {
            let seen = moved / span;
            self.velocity += (seen - self.velocity) * (FOLLOW * span).min(1.0);
            let climb = seen.dot(up);
            // A jump: the body rising faster by half a jump's speed at once (the simulation's
            // push is given in a tick; nothing on the ground speeds it up so).
            self.leapt = climb - self.climb > TAKE_OFF;
            let sinking = (self.climb - climb) / span;
            self.sinking += (sinking - self.sinking) * (FOLLOW * span).min(1.0);
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
        if span > 0.0 {
            let ahead = (self.velocity - before).dot(forward) / span;
            self.ahead += (ahead - self.ahead) * (FOLLOW * span).min(1.0);
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
            || self.leapt
            || (self.climb > TAKE_OFF && clear > LIFT_OFF)
            || (self.mode == Mode::Airborne && clear > LIFT_OFF && self.sinking > FREE_FALL);
        if flying {
            if self.mode != Mode::Airborne {
                // Leaving the ground it falls freely from then on: the push that sent it up is
                // not a fall slowing (measured, the leap would read so for a fifth of a second
                // and set it down at the top of its flight).
                self.sinking = 2.0 * FREE_FALL;
                self.flight = self.climb;
            }
            // Rising as a body in flight does: seen, its speed changes only as the simulation
            // ticks, a step at a time.
            self.flight -= 2.0 * FREE_FALL * dt;
            self.feet = None;
            upper::airborne(&mut turns);
            // The legs drawn up at the top of a flight, straight as they leave the ground and as
            // they meet it again, drawing up and letting down from rest.
            let rise = (self.flight / JUMP_SPEED).clamp(-1.0, 1.0);
            let tuck = (1.0 - rise * rise) * (1.0 - rise * rise);
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
            // Braking, the stride keeps its time, its steps shortened to where the body stops
            // (`travel` below): people stop in a step or so at about the pace they walked at
            // (Ridge et al.), not in ever slower steps.
            if self.ahead > -BRAKING || speed > self.pace {
                self.pace = speed;
            }
        } else if self.stepping && !self.stopping {
            // Stopping: the stride goes on, slowing, until the swinging foot lands beside the other.
            self.stopping = true;
        }
        let (pace, aim) = if moving {
            (self.pace, along)
        } else if self.stopping {
            // The last steps walked, both feet down between them (a run in place never has both
            // down to end on).
            (self.pace.clamp(SHUFFLE, STOP_PACE), DVec3::ZERO)
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
                // Setting off from standing: a foot lifts at once (half a stride on, the other is
                // planted), or it would stay behind as the body went on: the right; stepping into
                // place, the one standing furthest from where it would.
                let off = |s: usize| (feet[s].plant - rest[s].0).length();
                let first = if moving {
                    0
                } else {
                    usize::from(off(1) > off(0))
                };
                #[allow(clippy::cast_precision_loss)] // 0 or 1
                let half = 0.5 * first as f64;
                self.phase = (gait.duty + half).fract();
                self.stepping = true;
                for f in feet.iter_mut() {
                    f.duty = gait.duty;
                }
                // That first foot lifts flat: at rest, it had not rolled onto its toe.
                feet[first].from_rest = true;
            }
            // A planted foot left further behind than a steady stride leaves it (setting off, or
            // speeding up) hurries the stride, as people take quick steps then.
            let stride_time = 2.0 / gait.cadence;
            let mut late = 0.0f64;
            for (side, (f, s)) in feet.iter().zip(&spots).enumerate() {
                #[allow(clippy::cast_precision_loss)] // 0 or 1
                let into = (self.phase - 0.5 * side as f64).rem_euclid(1.0);
                // (Braking, feet land short on purpose: the stride does not hurry for them.)
                if f.down && into < f.duty && aim != DVec3::ZERO && self.ahead > -BRAKING {
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
        // Where the body will be `t` from now: at its speed, or braking, no further than it stops.
        let brake = (-self.ahead).max(0.0);
        let travel = |t: f64| {
            let s = aim.length();
            if s > 0.0 && brake > 0.0 {
                let t = t.min(s / brake);
                aim * (t - 0.5 * brake * t * t / s)
            } else {
                aim * t
            }
        };
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
            let landing = feet::shortened(&ground_at, spots[side], travel(left), up);
            lands_in[side] = to_land.max(0.0);
            if pace > 0.0 {
                (targets[side], throughs[side]) = feet[side].step(along_stride, &gait, landing, up);
            } else {
                targets[side] = (feet[side].plant, feet[side].normal);
            }
        }
        // Stopped once both feet stand where they would: the stride that brought them together is
        // done. A foot set down before the stop was asked for (ahead, as the body went on) steps
        // back in the same stride (an unplanned stop takes a little more than a step: Ridge et
        // al.), not after the body has stood.
        let placed = |(f, (g, _)): (&Foot, &(DVec3, DVec3))| (f.plant - *g).length() < SETTLE;
        if self.stopping && feet.iter().all(|f| f.down) && feet.iter().zip(&rest).all(placed) {
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
        // The hips lowered as the feet need (`hips.rs`).
        let reached = std::array::from_fn(|side| hips::Reached {
            ankle: ankles[side].0,
            through: throughs[side],
            aimed: feet[side].aimed,
            lands_in: lands_in[side],
        });
        let going = hips::Going {
            up,
            forward,
            along,
            ahead: self.ahead,
        };
        let stepping = pace > 0.0 && self.stepping;
        let needed = hips::needed(&root, &going, &reached, stepping).max(0.0);
        self.lower_hips(needed, fresh, dt);
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
