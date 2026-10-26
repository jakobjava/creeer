//! The hands holding a load (G1.4f, ADR-0027): a person's arms, not a rigid hold. They hold the
//! load's weight up and give it the acceleration of their planned path (lifting it, bringing it
//! in, swinging it round as the body turns), as people do from what they know of the load; and
//! they pull it towards where they mean to have it with an arm's stiffness and damping, as
//! measured (endpoint stiffness up to 445 N/m in an active task; damping ratios of 0.25–0.47,
//! Perreault and others: the more damped kept). The force is at most what the hands hold up
//! (`carry::STRENGTH`); the same goes back on the body at the hands. The load's turn is held to
//! the way the body faces by a torque as strong as the grip. So the body's bob on its legs and
//! its jerks reach the load softened, as through arms.
//!
//! Applied as the step's loads (taken off after it, as the legs' reactions are), both bodies
//! stepped as held: the arms' stiffness is kept below what the step can follow for a light load.

use super::carry::{TURNING, facing_frame};
use super::*;
use creeer_math::glam::{DMat3, DQuat};
use creeer_sim::carry;

/// The arms' stiffness (N/m) and damping ratio.
pub(super) const STIFFNESS: f64 = 445.0;
const DAMPING: f64 = 0.47;
/// The grip against a load's turn: the hands this far apart (m), each holding at most the
/// strength's half, and how far it turns before they give their most (rad).
const HANDS_APART: f64 = 0.4;
const GIVE_TURN: f64 = 0.1;
/// How far from where the hands mean to have it a load may be pulled before it is out of their
/// reach (m), and for how long they may give their most or have it there before it drops (a
/// knock is not a pull: a tenth of a second).
pub(super) const GIVE: f64 = 0.3;
const OVER: u32 = 6;
/// How fast the body turns the way it goes, at most (rad/s): as the drawn body turns
/// (`creeer_client::animate::stand`).
const TURN: f64 = 8.0;
/// The most a spring held as the step's load may ring at, times the step: kept so the step
/// follows it (for a light load the arms are softened to it).
const RING: f64 = 0.5;

/// The most torque the hands hold a load's turn with (N·m).
pub(super) fn grip() -> f64 {
    0.5 * carry::STRENGTH * 0.5 * HANDS_APART
}

/// A spring of `stiffness` on `mass` (or inertia), softened to ring at most `RING` a step, and
/// its damping at `DAMPING` of critical.
pub(super) fn spring(stiffness: f64, mass: f64) -> (f64, f64) {
    let k = stiffness.min(mass * (RING / DT) * (RING / DT));
    (k, 2.0 * DAMPING * (k * mass).sqrt())
}

impl ServerWorld {
    /// Before the step: each carrier's hands bring its load in and hold it the way the body
    /// faces (the way it goes, while it goes); one fallen, or whose load is gone, lets go.
    pub(super) fn carry_before(&mut self) {
        let carriers: Vec<EntityId> = self.carrying.keys().copied().collect();
        for by in carriers {
            let load = self.carrying.get(&by).map(|c| c.load);
            let (Some(i), Some(b)) = (self.index(by), load.and_then(|l| self.index(l))) else {
                self.let_go(by);
                continue;
            };
            if self.people.get(&by).is_some_and(people::Person::fallen) {
                self.let_go(by);
                continue;
            }
            self.hold(i, b, by);
        }
    }

    /// Avatar `by` (body `i`) holds its load (body `b`) this step.
    fn hold(&mut self, i: usize, b: usize, by: EntityId) {
        if let Some(end) = self.carrying.get(&by).and_then(|c| c.end) {
            self.hold_end(i, b, by, end);
            return;
        }
        let mass = self.mass_of(b);
        let s = &self.world.bodies[i].structure;
        let (centre, turn, v, w) = (
            s.position[0],
            s.orientation[0],
            s.velocity[0],
            s.angular_velocity[0],
        );
        let up = character::plumb(&self.world, centre);
        let gravity =
            (self.world.planet.as_ref()).map_or(-up * 9.806_65, |g| g.acceleration(centre));
        let c = self.carrying.get_mut(&by).expect("a carrier");
        // Turning the way it goes, no faster than the drawn body turns, and its swing round
        // (speeding up, α·r, and pulling in, ω²·r) within the hands' spare strength: as fast as
        // it can still stop where it is to face.
        let going = v.reject_from(up);
        let want = if going.length() > TURNING {
            going.normalize()
        } else {
            c.facing
        };
        let angle = c.facing.angle_between(want) * up.dot(c.facing.cross(want)).signum();
        let share = c.spare / (std::f64::consts::SQRT_2 * c.ahead.max(0.1));
        let (most, fastest) = (share, share.sqrt().min(TURN));
        let aim = (2.0 * most * angle.abs()).sqrt().min(fastest) * angle.signum();
        c.spin += (aim - c.spin).clamp(-most * DT, most * DT);
        c.facing = (DQuat::from_axis_angle(up, c.spin * DT) * c.facing).normalize();
        // Where the hands mean to have it: lifted to their height first, then brought in (not
        // dragged along the ground), in the body's frame.
        let hold = if c.one_hand {
            carry::at_side(centre, up, c.facing)
        } else {
            carry::held_out(centre, up, c.facing, c.ahead)
        };
        let held = turn.conjugate() * (hold - centre);
        let up_here = turn.conjugate() * up;
        let raised = c.from + up_here * (held - c.from).dot(up_here);
        let smooth = |x: f64| {
            let x = x.clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        // (Already where the hands hold it, a load has no way to go: none, not 0/0.)
        let x = if c.lift > 0.0 {
            2.0 * f64::from(c.since) * DT / c.lift
        } else {
            2.0
        };
        let at = if x < 1.0 {
            c.from.lerp(raised, smooth(x))
        } else {
            raised.lerp(held, smooth(x - 1.0))
        };
        c.since = c.since.saturating_add(1);
        // Its way from the body's centre, and that way's speed and acceleration as planned (in the
        // body's frame: the body's own sway is not planned).
        let off = turn * at;
        let rel_v = turn * c.path[0].map_or(DVec3::ZERO, |p| (at - p) / DT);
        let rel_a = turn
            * match c.path {
                [Some(p1), Some(p2)] => (at - 2.0 * p1 + p2) / (DT * DT),
                _ => DVec3::ZERO,
            };
        c.path = [Some(at), c.path[0]];
        let (target, target_v) = (centre + off, v + w.cross(off) + rel_v);
        // Turned from as taken to as held along the lift too (turned at once, a tool taken in one
        // hand spun at 30–60 rad/s about its target and a long one struck the ground).
        let turned = if c.one_hand {
            c.taken.slerp(c.turn, smooth(0.5 * x))
        } else {
            c.turn
        };
        let (held_at, keep) = (c.grip, facing_frame(c.facing, up) * turned);
        let spin = up * c.spin;
        let node = c.node;
        // The load where the hands hold it.
        let l = &self.world.bodies[b].structure;
        let (lc, lq, lv, lw) = (
            l.position[node],
            l.orientation[node],
            l.velocity[node],
            l.angular_velocity[node],
        );
        // Pulled at its centre of mass towards where it is when the grip is where the hands mean
        // it and the load turned as they keep it (G1.6d: pulled at the grip, a stick, whose grip
        // is off its axis and which barely resists turning about it, was spun up by the pull's
        // own lever).
        let grip_arm = keep * held_at;
        let (point, point_v) = (lc, lv);
        let (target, target_v) = (target - grip_arm, target_v - spin.cross(grip_arm));
        let (k, damp) = spring(STIFFNESS, mass);
        let mut force =
            mass * (rel_a - gravity) + (target - point) * k + (target_v - point_v) * damp;
        let strong = force.length() >= carry::STRENGTH;
        if strong {
            force *= carry::STRENGTH / force.length();
        }
        // Its turn kept to the way the body faces, as firmly as the grip: an angular spring of
        // one natural frequency on every axis, so the torque is the load's inertia (in the
        // world's frame) times the angular acceleration it asks. Its frequency is the grip's on
        // the load's largest moment, softened to ring at most `RING` a step: stable on every
        // axis however unequal they are (a stick's).
        let inertia = (self.world.bodies[b].shapes.first())
            .map_or(DMat3::IDENTITY * (mass * 0.01), |(_, s)| s.inertia(mass));
        let world_inertia = DMat3::from_quat(lq) * inertia * DMat3::from_quat(lq).transpose();
        let largest = inertia.x_axis.x.max(inertia.y_axis.y).max(inertia.z_axis.z);
        let omega = (grip() / GIVE_TURN / largest).sqrt().min(RING / DT);
        let off_turn = creeer_physics::so3::log(keep * lq.conjugate());
        let asked = off_turn * (omega * omega) + (spin - lw) * (2.0 * DAMPING * omega);
        let torque = (world_inertia * asked).clamp_length_max(grip());
        let c = self.carrying.get_mut(&by).expect("a carrier");
        c.straining = if strong { c.straining + 1 } else { 0 };
        c.torn = if (target - point).length() > GIVE {
            c.torn + 1
        } else {
            0
        };
        // On the load at the hands, and back on the body there: loads for this step alone.
        let on_load = (force, torque);
        // On the body at its centre: its weight and its pull, not their moment about it (given as
        // the step's load, the moment of a load held out set the body turning against its
        // balance, and the load with it: a limit).
        let on_body = (-force, DVec3::ZERO);
        for (body, node, (f, t)) in [(b, node, on_load), (i, 0, on_body)] {
            let held = self.world.bodies[body].held;
            self.world.bodies[body].wake();
            self.world.bodies[body].held = true;
            let s = &mut self.world.bodies[body].structure;
            s.load[node] += f;
            s.torque[node] += t;
            self.reactions.push(character::Reaction {
                body,
                node,
                force: f,
                torque: t,
                held,
                mass: None,
            });
        }
    }

    /// After the step: a load the hands have given their most for, or have had out of their
    /// reach, over a tenth of a second, drops.
    pub(super) fn carry_after(&mut self) {
        self.grip_holds_twist();
        let dropped: Vec<EntityId> = (self.carrying.iter())
            .filter(|(_, c)| c.straining >= OVER || c.torn >= OVER)
            .map(|(by, _)| *by)
            .collect();
        for by in dropped {
            self.let_go(by);
        }
    }
}

/// Below this share of its largest moment, a load's least is a handle's own axis: the fist
/// closes round it (a stick, a haft).
const SLENDER: f64 = 0.1;

impl ServerWorld {
    /// After the step: a slender load does not turn in the fist about its own axis beyond what
    /// the grip's friction lets (`grip`, as an impulse over the step: Coulomb's, it only takes
    /// spin away). The arms' spring cannot hold that axis (its moment is a few hundredths of a
    /// percent of the others: a stick's), and a stick pressed against a trunk or a crate by the
    /// hands was spun about it by its contact's friction, to 450–525 rad/s (G1.7).
    fn grip_holds_twist(&mut self) {
        let held: Vec<(usize, usize, DVec3)> = (self.carrying.iter())
            .filter_map(|(by, c)| {
                let (i, b) = (self.index(*by)?, self.index(c.load)?);
                let centre = self.world.bodies[i].structure.position[0];
                let up = character::plumb(&self.world, centre);
                Some((b, c.node, up * c.spin))
            })
            .collect();
        for (b, node, spin) in held {
            let mass = self.mass_of(b);
            let Some(inertia) = (self.world.bodies[b].shapes.first()).map(|(_, s)| s.inertia(mass))
            else {
                continue;
            };
            let moments = [inertia.x_axis.x, inertia.y_axis.y, inertia.z_axis.z];
            let largest = moments.iter().copied().fold(0.0, f64::max);
            let (k, least) = (moments.iter().copied().enumerate())
                .fold((0, f64::MAX), |m, (k, v)| if v < m.1 { (k, v) } else { m });
            if !(least > 0.0 && least < SLENDER * largest) {
                continue;
            }
            let s = &mut self.world.bodies[b].structure;
            let axis = s.orientation[node] * DVec3::AXES[k];
            let twist = (s.angular_velocity[node] - spin).dot(axis);
            let most = grip() * DT / least;
            s.angular_velocity[node] -= axis * twist.clamp(-most, most);
        }
    }
}
