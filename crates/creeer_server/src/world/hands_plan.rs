//! The hands' plan of where they bring what they hold (G2.10, ADR-0036; split from `hands.rs`):
//! aimed by the player, straight to where it is aimed at the hands' speed; by themselves,
//! critically damped within their spare strength.
//!
//! Aimed, its turn is planned no faster than the hands' torque turns it and stops it (G2.11 B2):
//! turned at the hands' 8 rad/s whatever it weighs, a 3 m stringer or pole carried before the
//! chest and turned along the view (Q) swung past where it was aimed and back, 90° to 23° and
//! out to 65° the other way in two seconds (the grip's turn springs, capped at the hands'
//! torque, can neither keep up with such a plan nor stop the load where it ends).

use super::super::carry::{Carry, Plan};
use super::*;
use creeer_math::glam::DQuat;
use creeer_sim::carry;

/// How far the hands' plan leads what they hold at most (m), and its turn (rad).
const LEAD: f64 = 0.25;
const LEAD_TURN: f64 = 0.5;

impl ServerWorld {
    /// Carrier `c`'s plan put at `(at, turn)` (from the body's centre): the load aimed by the
    /// player goes where it is aimed, its plan's speed the change; lying, its turn as fast as the
    /// hands' torque (`torque`, N·m) turns its moment (`inertia`) and stops it there, never more
    /// than [`LEAD_TURN`] ahead of the turn it has (`held_turn`). Returns no acceleration to hold
    /// up beyond its weight (a step's change is no acceleration the hands plan).
    pub(super) fn directly(
        c: &mut Carry,
        (at, turn): (DVec3, DQuat),
        (held_turn, lying): (DQuat, bool),
        (torque, inertia): (f64, f64),
    ) -> DVec3 {
        // As fast as hands move and turn a thing aimed, no faster: stood up (R) at once, a pole
        // was swung round at 94 rad/s and knocked its holder down.
        let (at, aimed, velocity, aimed_spin) =
            carry::aimed_step((c.plan.at, c.plan.turn), (at, turn), DT);
        // Stood up, as before: the client lifts a post clear to turn by how far it has turned as
        // drawn, at the hands' speed, and a post turned slower than drawn met the ground and
        // never stood (`build_flow_tests.rs`).
        if !lying {
            c.plan = Plan {
                at,
                velocity,
                turn: aimed,
                spin: aimed_spin,
                there: true,
            };
            return DVec3::ZERO;
        }
        // The turn: at most the speed from which half the hands' torque stops it in the turn
        // left, and never far from the load's own (the god-object's rule).
        let alpha = (0.5 * torque / inertia.max(1e-6)).max(0.5);
        let ahead = creeer_physics::so3::log(turn * c.plan.turn.conjugate());
        let most = (2.0 * alpha * ahead.length()).sqrt().min(carry::AIMED_TURN);
        let spin = (ahead / DT).clamp_length_max(most);
        let mut planned = (creeer_physics::so3::exp(spin * DT) * c.plan.turn).normalize();
        let lead = creeer_physics::so3::log(planned * held_turn.conjugate());
        if lead.length() > LEAD_TURN {
            planned = (creeer_physics::so3::exp(lead * (LEAD_TURN / lead.length())) * held_turn)
                .normalize();
        }
        c.plan = Plan {
            at,
            velocity,
            turn: planned,
            spin,
            there: true,
        };
        DVec3::ZERO
    }

    /// Carrier `c`'s plan one step on towards `(target, turn)`: critically damped, its
    /// acceleration within what its spare strength gives a load of `mass`, its speed and turn
    /// within the hands', and never more than [`LEAD`] ahead of the point `held` (where the
    /// load is held now). Returns the plan's acceleration.
    pub(super) fn planned(
        c: &mut Carry,
        (target, turn, held, held_turn): (DVec3, DQuat, DVec3, DQuat),
        mass: f64,
        (gravity, (torque, inertia)): (DVec3, (f64, f64)),
    ) -> DVec3 {
        let w = 2.0 * std::f64::consts::PI * carry::PLAN_HZ;
        let p = &mut c.plan;
        let spare = carry::plan_spare(mass, gravity.length(), c.one_hand);
        let mut a = (target - p.at) * (w * w) - p.velocity * (2.0 * w);
        a = a.clamp_length_max(spare);
        p.velocity = (p.velocity + a * DT).clamp_length_max(carry::FASTEST);
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
        p.spin = (p.spin + alpha * DT).clamp_length_max(carry::TURNING_AT_MOST);
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
}
