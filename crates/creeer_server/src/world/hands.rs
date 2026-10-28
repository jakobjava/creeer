//! The hands holding a load (G2.10, ADR-0036): a grip solved with the physics
//! (`creeer_physics::world::grip`) from the point held to where the hands mean it, as stiff as
//! a hand moving what it means to (critically damped at 8 Hz for the load's mass and moment:
//! settled in about a tenth of a second whatever it weighs), its pull at most the hands'
//! strength and its torque at most the wrist's (one hand) or the two hands' grip. Where they
//! mean it: where the player steers it (`Carry::input`), held still in the world (steady), or
//! by themselves before the body (elbow height, a forearm out; a tool by the side). The hands
//! bring it there along a plan that is critically damped too and no faster than their spare
//! strength accelerates it, its weight and that acceleration held up as planned; the body bears
//! their pull back. A load too heavy lags or sags; one torn out of their reach for half a second
//! drops. (G1.4f's soft arms, 445 N/m applied as the step's load, rang a 15 kg box at 0.87 Hz:
//! the user saw what they held float, lag, spin and pass through them.)

use super::carry::{Carry, TURNING, WRIST, facing_frame};
use super::*;
use creeer_math::glam::{DMat3, DQuat};
use creeer_physics::world::grip::Grip;
use creeer_physics::world::joint::ARM;
use creeer_sim::{carry, reach};

/// The grip's frequency (Hz) and the hands' plan's (Hz): the grip follows the plan within a
/// tenth of a second, the plan where they mean it within about a third.
const HZ: f64 = 8.0;
const PLAN_HZ: f64 = 3.0;
/// The hands' speed at most (m/s: peak hand speeds in reaching, 1.5–2 m/s) and how fast they
/// turn what they hold at most (rad/s).
const FASTEST: f64 = 2.0;
const TURNING_AT_MOST: f64 = 6.0;
/// How far the hands' planned turn leads the load's at most (rad).
const LEAD_TURN: f64 = 0.5;
/// The farthest the grip is from the shoulders (m: forward grip reach about 0.7 m).
const REACH: f64 = 0.75;
/// How far the hands' plan leads what they hold at most (m).
const LEAD: f64 = 0.25;
/// How far beyond the arms' reach of the shoulders the point held may be (m: the body moved
/// away from it, or it was pulled away) for how many ticks (half a second) before it drops.
const OUT_OF_REACH: f64 = 0.35;
const TORN: u32 = 30;
/// How far the shoulders come down and out towards what the hands reach below them, stooping
/// and squatting (m): from 0.818 of the stature standing to about 0.43 of it squatting, the
/// trunk leant forward (Winter's segments: a 1.75 m person's shoulders 0.7 m lower, 0.35 m on),
/// so the hands reach down to the ground just before the feet.
const STOOP_DOWN: f64 = 0.7;
const STOOP_OUT: f64 = 0.35;
/// How fast the body turns the way it goes, at most (rad/s): as the drawn body turns.
const TURN: f64 = 8.0;
/// The two hands this far apart (m), each holding at most half the strength.
const HANDS_APART: f64 = 0.4;
/// Below this share of its largest moment a load's least is a pole's own axis: held at its
/// end and too long to follow the hands, it is moved along hand over hand at this speed (m/s).
pub(super) const SLENDER: f64 = 0.1;
const HAND_OVER_HAND: f64 = 0.5;
/// Pushing a slender load forward this hard (N) and it does not go: hand over hand along it.
const PUSHING: f64 = 40.0;

/// The most torque two hands hold a load's turn with (N·m).
pub(super) fn grip() -> f64 {
    0.5 * carry::STRENGTH * 0.5 * HANDS_APART
}

impl ServerWorld {
    /// Before the step: each carrier's hands grip their load where they mean it; one fallen,
    /// or whose load is gone, lets go.
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

    /// Avatar `by` (body `i`) grips its load (body `b`) this step.
    fn hold(&mut self, i: usize, b: usize, by: EntityId) {
        let mass = self.mass_of(b);
        let s = &self.world.bodies[i].structure;
        let (centre, v) = (s.position[0], s.velocity[0]);
        let up = character::plumb(&self.world, centre);
        let gravity =
            (self.world.planet.as_ref()).map_or(-up * 9.806_65, |g| g.acceleration(centre));
        let inertia = (self.world.bodies[b].shapes.first())
            .map_or(DMat3::IDENTITY * (mass * 0.01), |(_, s)| s.inertia(mass));
        let moments = [inertia.x_axis.x, inertia.y_axis.y, inertia.z_axis.z];
        let largest = moments.iter().copied().fold(0.0, f64::max);
        let least = moments.iter().copied().fold(f64::MAX, f64::min);
        // The other hand at work (fastening what this one holds steady): held in one.
        let busy = self.building.under_way.contains_key(&by);
        let c = self.carrying.get_mut(&by).expect("a carrier");
        // The body turns the way it goes, no faster than it is drawn turning.
        let going = v.reject_from(up);
        if going.length() > TURNING {
            let want = going.normalize();
            let angle = c.facing.angle_between(want) * up.dot(c.facing.cross(want)).signum();
            let step = angle.clamp(-TURN * DT, TURN * DT);
            c.facing = (DQuat::from_axis_angle(up, step) * c.facing).normalize();
        }
        let (target, turn) = Self::meant(c, centre, up);
        let l = &self.world.bodies[b].structure;
        let held = l.position[c.node] + l.orientation[c.node] * c.grip;
        let turning = (if c.one_hand || busy { WRIST } else { grip() }, largest);
        let held_turn = l.orientation[c.node];
        let plan = Self::planned(
            c,
            (target - centre, turn, held - centre, held_turn),
            mass,
            (gravity, turning),
        );
        let one_hand = c.one_hand || busy;
        let strength = if one_hand {
            0.5 * carry::STRENGTH
        } else {
            carry::STRENGTH
        };
        // Its weight and the plan's acceleration held up as planned: the target raised by
        // what the spring then pulls with (capped with the rest).
        let w = 2.0 * std::f64::consts::PI * HZ;
        let k = mass * w * w;
        let turn_k = largest * w * w / (2.0 * ARM * ARM);
        let grip = Grip {
            a: (b, c.node),
            at_a: c.grip,
            target: centre + c.plan.at + (plan - gravity) * (mass / k),
            target_velocity: v + c.plan.velocity,
            turn: c.plan.turn,
            spin: c.plan.spin,
            stiffness: k,
            damping: 2.0 * mass * w,
            most: strength,
            turn_stiffness: turn_k,
            turn_damping: 2.0 * turn_k / w,
            most_torque: if one_hand { WRIST } else { grip() },
        };
        let (node, pull) = (c.node, c.pull);
        // A pole held at its end too long to follow: the hands move along it.
        if least < SLENDER * largest {
            let l = &self.world.bodies[b].structure;
            let (lc, lq) = (l.position[node], l.orientation[node]);
            let k_axis = (0..3).fold(0, |m, k| if moments[k] < moments[m] { k } else { m });
            let axis = DVec3::AXES[k_axis];
            let half =
                (self.world.bodies[b].shapes.first()).map_or(0.0, |(_, s)| s.extent(lq, lq * axis));
            let c = self.carrying.get_mut(&by).expect("a carrier");
            let held = lc + lq * c.grip;
            // Lagging behind where the hands mean to be, the way the body faces (walked under
            // it, a pole's end can rise no higher than the hands reach): they move along it
            // that way, hand over hand. (Towards the point of it nearest where they meant to be,
            // they stayed at the very end of a pole raised overhead, and it stalled at 42°.)
            let gap = (target - held).dot(c.facing);
            // (Or pushing it forward and it does not go: 200 N into a pole whose end could rise
            // no higher held the walker in place, the gap closed.)
            let push = c.pull.dot(c.facing);
            if gap.abs() > 0.05 || push > PUSHING {
                let ahead = if push > PUSHING { 1.0 } else { gap.signum() };
                let way = (lq * axis).dot(c.facing).signum() * ahead;
                let next = c.grip.dot(axis) + way * HAND_OVER_HAND * DT;
                c.grip = axis * next.clamp(-half, half) + c.grip.reject_from(axis);
            }
        }
        let grip = Grip {
            at_a: self.carrying.get(&by).map_or(grip.at_a, |c| c.grip),
            ..grip
        };
        self.world.add_grip(grip);
        // The body bears the hands' last pull back, at its centre (its moment about the body
        // is not on it: given as the step's load, it set the body turning against its balance).
        let held = self.world.bodies[i].held;
        self.world.bodies[i].wake();
        self.world.bodies[i].held = true;
        self.world.bodies[i].structure.load[0] -= pull;
        self.reactions.push(character::Reaction {
            body: i,
            node: 0,
            force: -pull,
            torque: DVec3::ZERO,
            held,
            mass: None,
        });
        self.world.bodies[b].held = true;
    }

    /// Where carrier `c`'s hands mean the grip, and the load's turn: as the player steers it
    /// (held still where it was when steadied), or by themselves before the body (by the side,
    /// a tool); within the arms' reach of the shoulders.
    fn meant(c: &mut Carry, centre: DVec3, up: DVec3) -> (DVec3, DQuat) {
        let (target, turn) = match c.input {
            Some(h) if h.steady => *c.steady.get_or_insert((centre + c.plan.at, c.plan.turn)),
            // (The player steers the load's own place and turn; the hands hold it at the grip.)
            Some(h) => (centre + h.offset + h.turn * c.grip, h.turn),
            None => {
                let at = if c.tool {
                    carry::at_side(centre, up, c.facing)
                } else {
                    carry::held_out(centre, up, c.facing, c.ahead)
                };
                (at, facing_frame(c.facing, up) * c.turn)
            }
        };
        if !c.input.is_some_and(|h| h.steady) {
            c.steady = None;
        }
        // Clear of the body: at least as far out from its axis as it was found to need when
        // taken (`Carry::ahead`). Held things pass the body's box (one box is no chest and
        // arms: a 30 kg box held against it rubbed along it, and was carried at 0.40 m/s, not
        // 1.2), so the hands keep them out of it.
        let out = (target - centre).reject_from(up);
        let target = if out.length() < c.ahead && !c.tool {
            let way = out.try_normalize().unwrap_or(c.facing);
            target + way * (c.ahead - out.length())
        } else {
            target
        };
        let shoulders = stooped(reach::shoulders(centre, up), up, target);
        let off = target - shoulders;
        let target = if off.length() > REACH {
            shoulders + off * (REACH / off.length())
        } else {
            target
        };
        (target, turn)
    }

    /// Carrier `c`'s plan one step on towards `(target, turn)`: critically damped, its
    /// acceleration within what its spare strength gives a load of `mass`, its speed and turn
    /// within the hands', and never more than [`LEAD`] ahead of the point `held` (where the
    /// load is held now). Returns the plan's acceleration.
    fn planned(
        c: &mut Carry,
        (target, turn, held, held_turn): (DVec3, DQuat, DVec3, DQuat),
        mass: f64,
        (gravity, (torque, inertia)): (DVec3, (f64, f64)),
    ) -> DVec3 {
        let w = 2.0 * std::f64::consts::PI * PLAN_HZ;
        let p = &mut c.plan;
        let strength = if c.one_hand { 0.5 } else { 1.0 } * carry::STRENGTH;
        // The hands' spare strength over the load and the arms (`carry::spare`'s law).
        let spare = (0.5 * (strength - mass * gravity.length()) / (mass + carry::ARMS)).max(0.5);
        let mut a = (target - p.at) * (w * w) - p.velocity * (2.0 * w);
        a = a.clamp_length_max(spare);
        p.velocity = (p.velocity + a * DT).clamp_length_max(FASTEST);
        p.at += p.velocity * DT;
        // The hands stay with what they hold: a load that cannot follow (too heavy, caught,
        // its end on the ground) keeps the plan within reach of it (planned on regardless, a
        // pole lifted by its end fell behind the plan and was dropped for "out of reach").
        let ahead = p.at - held;
        if ahead.length() > LEAD {
            let way = ahead / ahead.length();
            p.at = held + way * LEAD;
            p.velocity -= way * p.velocity.dot(way).max(0.0);
        }
        // Its turn alike: critically damped, its angular acceleration within half of what the
        // hands' torque gives its moment, and never more than [`LEAD_TURN`] ahead of the turn it
        // has. (Turned at a set rate whatever it weighs, a branch held in one hand fell far
        // behind as the body turned to walk off, and the grip swung it round at 14 rad/s.)
        let alpha_most = (0.5 * torque / inertia.max(1e-6)).max(0.5);
        let off = creeer_physics::so3::log(turn * p.turn.conjugate());
        let alpha = (off * (w * w) - p.spin * (2.0 * w)).clamp_length_max(alpha_most);
        p.spin = (p.spin + alpha * DT).clamp_length_max(TURNING_AT_MOST);
        p.turn = (creeer_physics::so3::exp(p.spin * DT) * p.turn).normalize();
        let lead = creeer_physics::so3::log(p.turn * held_turn.conjugate());
        if lead.length() > LEAD_TURN {
            let way = lead / lead.length();
            p.turn = (creeer_physics::so3::exp(way * LEAD_TURN) * held_turn).normalize();
            p.spin -= way * p.spin.dot(way).max(0.0);
        }
        p.there = (target - p.at).length() < 0.03 && (p.at - held).length() < 0.05;
        a
    }

    /// After the step: each carrier's hands' pull (the body bears it next step), and a load
    /// out of their reach for half a second drops; the grips come off.
    pub(super) fn carry_after(&mut self) {
        self.grip_holds_twist();
        let carriers: Vec<EntityId> = self.carrying.keys().copied().collect();
        let mut dropped = Vec::new();
        for by in carriers {
            let Some(b) = self.carrying.get(&by).and_then(|c| self.index(c.load)) else {
                continue;
            };
            let Some(grip) = self.world.grips().iter().find(|g| g.a.0 == b).copied() else {
                continue;
            };
            let (force, _) = grip.pull(&self.world);
            let s = &self.world.bodies[b].structure;
            let held = s.position[grip.a.1] + s.orientation[grip.a.1] * grip.at_a;
            let shoulders = self.index(by).map_or(held, |i| {
                let centre = self.world.bodies[i].structure.position[0];
                let up = character::plumb(&self.world, centre);
                stooped(reach::shoulders(centre, up), up, held)
            });
            let c = self.carrying.get_mut(&by).expect("a carrier");
            c.pull = force;
            let from = (held - shoulders).length();
            c.reach = c.reach.min(from.max(REACH));
            c.torn = if from > c.reach + OUT_OF_REACH {
                c.torn + 1
            } else {
                0
            };
            if c.torn >= TORN {
                dropped.push(by);
            }
        }
        self.world.clear_grips();
        for by in dropped {
            self.let_go(by);
        }
    }
}

/// The `shoulders` (standing, upright along `up`) as near `target` as stooping and squatting
/// bring them: down to it by [`STOOP_DOWN`] at most, and out towards it as they come down.
fn stooped(shoulders: DVec3, up: DVec3, target: DVec3) -> DVec3 {
    let off = target - shoulders;
    let down = (-off.dot(up)).clamp(0.0, STOOP_DOWN);
    let out = off.reject_from(up);
    let on = out.length().min(STOOP_OUT * down / STOOP_DOWN);
    shoulders - up * down + out.normalize_or_zero() * on
}
