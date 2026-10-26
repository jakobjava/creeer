//! Raising a long thing by one end (G2.9): what the hands cannot hold level (its weight about
//! the grip is more than the grip's torque) they hold by one point only, the other end resting
//! where it lies, as people lift a log's end, drag it, or walk a post up: the hands at about head
//! height, moving along it hand over hand towards the way the body goes. A post's foot set in a
//! hole first is walked up until it stands, and then backfilled (`posts.rs`).
//!
//! The force is the arms' (`hands.rs`: their stiffness, damping and strength) at the point held,
//! with the share of its weight a beam resting on its far end puts there held up as planned; no
//! torque: a point held turns as it will about the hands.

use super::carry::{Carry, TURNING};
use super::hands::{GIVE, STIFFNESS, spring};
use super::*;
use creeer_sim::carry;

/// How far above the elbows the hands hold an end raised (m): about head height (a man's elbows
/// stand about 1.09 m up, his head about 1.75 m; Pheasant's tables).
const RAISED: f64 = 0.6;
/// How fast the hands move along it, hand over hand (m/s): a grip let go and taken again a
/// hand's width on, a few times a second.
const HAND_OVER_HAND: f64 = 0.5;
/// How far the end may lag where the hands mean to have it before they wait for it (m).
const WAIT: f64 = 0.1;

/// A load held by one point: its long axis in its node's frame (unit) and its half length (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct End {
    pub(super) axis: DVec3,
    pub(super) half: f64,
}

impl ServerWorld {
    /// How body `load` is held at `point`, if by one end: a lone node (a log, a pole, a plank)
    /// whose weight about the grip is more than the hands' grip holds level.
    pub(super) fn end_hold(&self, load: usize, point: DVec3) -> Option<End> {
        let body = &self.world.bodies[load];
        let s = &body.structure;
        if s.node_count() != 1 {
            return None;
        }
        let (c, q) = (s.position[0], s.orientation[0]);
        let mass = self.mass_of(load);
        let (_, shape) = body.shapes.first()?;
        let inertia = shape.inertia(mass);
        let moments = [inertia.x_axis.x, inertia.y_axis.y, inertia.z_axis.z];
        let (k, _) = (moments.iter().enumerate())
            .fold((0, f64::MAX), |m, (k, &v)| if v < m.1 { (k, v) } else { m });
        let axis = DVec3::AXES[k];
        let half = shape.extent(q, q * axis);
        let up = character::plumb(&self.world, c);
        let lever = (point - c).reject_from(up).length();
        let g = (self.world.planet.as_ref()).map_or(9.806_65, |p| p.acceleration(c).length());
        (mass * g * lever > super::hands::grip()).then_some(End { axis, half })
    }

    /// Avatar `by` (body `i`) holds its load (body `b`) by one point this step.
    pub(super) fn hold_end(&mut self, i: usize, b: usize, by: EntityId, end: End) {
        let mass = self.mass_of(b);
        let s = &self.world.bodies[i].structure;
        let (centre, turn, v) = (s.position[0], s.orientation[0], s.velocity[0]);
        let up = character::plumb(&self.world, centre);
        let gravity =
            (self.world.planet.as_ref()).map_or(-up * 9.806_65, |g| g.acceleration(centre));
        let l = &self.world.bodies[b].structure;
        let (lc, lq, lv, lw) = (
            l.position[0],
            l.orientation[0],
            l.velocity[0],
            l.angular_velocity[0],
        );
        let c: &mut Carry = self.carrying.get_mut(&by).expect("a carrier");
        let going = v.reject_from(up);
        if going.length() > TURNING {
            c.facing = going.normalize();
        }
        // Where the hands mean to have it: from where they took it up to head height before the
        // body, over the lift.
        let raised = carry::held_out(centre, up, c.facing, carry::forearm()) + up * RAISED;
        let x = (f64::from(c.since) * DT / c.lift.max(DT)).clamp(0.0, 1.0);
        let at = c.from.lerp(
            turn.conjugate() * (raised - centre),
            x * x * (3.0 - 2.0 * x),
        );
        let target = centre + turn * at;
        // Hand over hand along it, towards the point of it nearest where they mean to be.
        let axis = lq * end.axis;
        let now = c.grip.dot(end.axis);
        let want = (target - lc).dot(axis).clamp(-end.half, end.half);
        let along = now + (want - now).clamp(-HAND_OVER_HAND * DT, HAND_OVER_HAND * DT);
        c.grip = end.axis * along;
        let arm = lq * c.grip;
        let (point, point_v) = (lc + arm, lv + lw.cross(arm));
        // The plan goes on only while the end keeps up: the hands wait for a heavy end, as
        // people lift one (planned on regardless, the end fell behind out of the arms' reach,
        // G2.9).
        if (target - point).length() < WAIT {
            c.since = c.since.saturating_add(1);
        }
        // The share of its weight the hands hold, as a beam resting on its far end does: its
        // centre's lever about that end over the hands', across the vertical.
        let foot = lc - axis * (end.half * along.signum());
        let lever = |p: DVec3| (p - foot).reject_from(up).length();
        let share = (lever(lc) / lever(point).max(1e-3)).clamp(0.0, 1.0);
        let (k, damp) = spring(STIFFNESS, mass);
        let mut force = -share * mass * gravity + (target - point) * k + (v - point_v) * damp;
        let strong = force.length() >= carry::STRENGTH;
        if strong {
            force *= carry::STRENGTH / force.length();
        }
        c.straining = if strong { c.straining + 1 } else { 0 };
        c.torn = if (target - point).length() > GIVE {
            c.torn + 1
        } else {
            0
        };
        // At the point held: its force, and its moment about the node.
        let on_load = (force, arm.cross(force));
        let on_body = (-force, DVec3::ZERO);
        for (body, (f, t)) in [(b, on_load), (i, on_body)] {
            let held = self.world.bodies[body].held;
            self.world.bodies[body].wake();
            self.world.bodies[body].held = true;
            let s = &mut self.world.bodies[body].structure;
            s.load[0] += f;
            s.torque[0] += t;
            self.reactions.push(character::Reaction {
                body,
                node: 0,
                force: f,
                torque: t,
                held,
                mass: None,
            });
        }
    }
}
