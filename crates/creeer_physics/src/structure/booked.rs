//! The energy guard's books (`group.rs`): the work done on a group's members in a substep that
//! the energy's potential of the loads does not count, and what each scheme carries of the
//! loads into the next substep.

use super::Structure;
use super::group::Member;
use super::solver::{ImplicitSolver, Scheme};
use creeer_math::glam::{DQuat, DVec3};

/// Where a member started a substep, and its dashpots' forces then.
pub(super) type Start = (Vec<DVec3>, Vec<DQuat>, Vec<(usize, DVec3, DVec3)>);

/// The work the members' external loads and torques did since they were at `start` (J) that
/// the energy's potential of the loads does not count. A torque has no potential as a load
/// has (rotations do not add): its work, τ·Δθ for the rotation vector Δθ a node turned by,
/// is credited instead; without that every joule a torque gave was taken for the
/// integration's gain and removed (a motor in a free-floating station turned it for a
/// second, then no more: F6). Newmark's forces are the mean of the step's two ends: a load
/// or torque that changed did the work of its mean with the last step's, which the potential
/// (at the load now) misses by ½·(f₋₁ − f)·Δx: unbooked, a thruster or motor switched on or
/// off took the step for a gain and redid it by backward Euler.
///
/// BDF2 carries a third of the last step's change of momentum into this one (v̂ = (4v − v₋₁)/3),
/// so of the loads it gives (g₋₁ + 2f)/3, g₋₁ their share of the last step's (`given`): the
/// potential misses (g₋₁ − f)/3·Δx. A character's walk is such a load, set anew each tick
/// (its stride's force closes a fifteenth of the gap to the speed asked for each step):
/// unbooked, a walk from rest on level ground gained up to 0.11 J a step, and every other step
/// was redone by backward Euler. Booked, it gains at most 5 mJ (its second-order error,
/// 3·10⁻⁴ of its kinetic energy), on the level and on 10° slopes, and none is redone (ADR-0024,
/// "G2.0: the standing character's books").
pub(super) fn unbooked_work(members: &[Member<'_>], start: &[Start], scheme: Scheme) -> f64 {
    let mut work = 0.0;
    for ((s, v), (x0, q0, dashpots)) in members.iter().zip(start) {
        for i in 0..s.node_count() {
            let (torque, before) = (s.torque[i], v.previous_torque[i]);
            let torque = match scheme {
                Scheme::Newmark => 0.5 * (torque + before),
                Scheme::Bdf2 => (v.given[1][i] + 2.0 * torque) / 3.0,
                Scheme::BackwardEuler => torque,
            };
            if torque != DVec3::ZERO {
                work += torque.dot(crate::so3::log(s.orientation[i] * q0[i].conjugate()));
            }
            let missed = match scheme {
                Scheme::Newmark => 0.5 * (v.previous_load[i] - s.load[i]),
                Scheme::Bdf2 => (v.given[0][i] - s.load[i]) / 3.0,
                Scheme::BackwardEuler => DVec3::ZERO,
            };
            if missed != DVec3::ZERO {
                work += missed.dot(s.position[i] - x0[i]);
            }
        }
        work += s.dashpot_work(x0, q0) + s.added_work(x0, v.h);
        // Newmark's mean took the last step's dashpots' forces too (in its accelerations).
        if scheme == Scheme::Newmark {
            for &(n, force, torque) in dashpots {
                work += 0.5
                    * (force.dot(s.position[n] - x0[n])
                        + torque.dot(crate::so3::log(s.orientation[n] * q0[n].conjugate())));
            }
        }
    }
    work
}

/// Whether a member's loads do work the loads' potential misses (`unbooked_work`).
pub(super) fn acting(s: &Structure, v: &ImplicitSolver) -> bool {
    s.torque
        .iter()
        .chain(&v.previous_torque)
        .any(|t| *t != DVec3::ZERO)
        || s.load != v.previous_load
        || v.given != [&s.load[..], &s.torque[..]]
        || !s.dashpots.is_empty()
        || !s.added_mass.is_empty()
}

/// The loads' and torques' share of the change of momentum of the substep just taken by
/// `scheme` (`given`): backward Euler's is the loads, Newmark's the mean with the last step's,
/// BDF2's a third of the last share and two thirds of the loads. Within rounding of the loads
/// it is the loads (the books then need nothing of the substep's start). After a history
/// reset it is the loads (`initial_accelerations`): the reset's velocities carry no change
/// of momentum, and BDF2's next step gives every force two thirds, the loads and the
/// contacts holding them alike. Taken as nought there, a load pressing a pole on the ground
/// was booked at two thirds against its contacts' whole, and the guard redid the step.
pub(super) fn give(s: &Structure, v: &mut ImplicitSolver, scheme: Scheme) {
    let ImplicitSolver {
        given,
        previous_load,
        previous_torque,
        ..
    } = v;
    let before = [&*previous_load, &*previous_torque];
    for (k, now) in [&s.load, &s.torque].into_iter().enumerate() {
        for (i, g) in given[k].iter_mut().enumerate() {
            let next = match scheme {
                Scheme::BackwardEuler => now[i],
                Scheme::Bdf2 => (*g + 2.0 * now[i]) / 3.0,
                Scheme::Newmark => 0.5 * (now[i] + before[k][i]),
            };
            *g = if (next - now[i]).length() <= 1e-12 * now[i].length() + 1e-9 {
                now[i]
            } else {
                next
            };
        }
    }
}
