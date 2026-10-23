//! Stepping one group of touching bodies: the scheme for its contacts, Newton's impact law,
//! and impact substeps for structures that deform in a crash.
//!
//! An implicit step spreads the force that stops a body over the whole step: stopping a
//! 1 t mass from 20 m/s within 1/60 s takes 1.2 MN on average, but within the 30 ms a
//! crash really lasts the force is capped by what the members can carry before they yield.
//! A 60 Hz step thus never reaches the yield load of a stiff member: the energy would
//! vanish in the step (backward Euler's numerical damping) instead of crushing the
//! structure. So a group whose structure takes an impact is stepped in substeps of at most
//! 0.5 ms until it no longer deforms plastically: the crash then does its plastic work.
//! The substep resolves the ringing of a light joint against a wall (a period of 2–3 ms);
//! measured against the analytic crush (`crush_tests.rs`), 1 ms is 7 % short at 5 m/s,
//! 0.5 ms within 1 % from 5 to 20 m/s, and smaller steps change it by under 2 %. Rigid
//! bodies need no substeps: the impact law is exact for them.

use super::contact::{Contacts, Side};
use super::{Body, anchor, impact};
use crate::structure::Scheme;
use crate::structure::group::{Coupling, MemberState, step_group_with};

/// Longest time step of a deforming impact (s).
pub const IMPACT_STEP: f64 = 0.5e-3;

/// The plastic strain rate (1/s) above which a structure struck in an impact goes on being
/// stepped in impact substeps. A crash makes its members flow at 3–10 /s (a tube crushed at
/// 5–20 m/s); a structure sagging under a load, as the sandbox bridge under the 10 t block
/// that landed on it, at 0.4 /s at most, which 60 Hz steps follow: its final sag and plastic
/// strain within 1 % of 0.5 ms substeps throughout (`crush_tests.rs`), at half the cost.
pub const FAST_FLOW: f64 = 1.0;

/// One group's share of a step: its bodies (borrowed from the world), its contacts, its
/// friction anchors afterwards, and the global indices of its bodies.
pub(super) type GroupWork<'a> = (
    Vec<&'a mut Body>,
    Contacts,
    Vec<anchor::Anchor>,
    &'a Vec<usize>,
);

/// Total energy of a group: its bodies' plus the contacts' elastic energy (J).
pub(super) fn group_energy(bodies: &[&mut Body], contacts: &Contacts) -> f64 {
    let states: Vec<MemberState<'_>> = bodies
        .iter()
        .map(|b| MemberState {
            position: &b.structure.position,
            orientation: &b.structure.orientation,
        })
        .collect();
    bodies.iter().map(|b| b.structure.energy()).sum::<f64>() + contacts.elastic_energy(&states)
}

/// The gaps of a group's contacts as measured (`Contacts::measured_gaps`).
fn measured_gaps(bodies: &[&mut Body], contacts: &Contacts) -> Vec<f64> {
    let states: Vec<MemberState<'_>> = bodies
        .iter()
        .map(|b| MemberState {
            position: &b.structure.position,
            orientation: &b.structure.orientation,
        })
        .collect();
    contacts.measured_gaps(&states)
}

/// Advances a group by `dt`.
pub(super) fn step(bodies: &mut [&mut Body], contacts: &mut Contacts, dt: f64) {
    let impact = contacts.list.iter().any(impact::is_impact);
    // An impact's own books leave out the contacts found pressed in at its start (`substep`).
    if impact {
        contacts.set_left(None);
    }
    let deformable = bodies.iter().any(|b| b.structure.element_count() > 0);
    let striking = impact && contacts.list.iter().any(|c| can_yield(bodies, c));
    if deformable && (striking || bodies.iter().any(|b| b.crushing)) {
        crush(bodies, contacts, dt);
    } else {
        match joint_substeps(bodies, contacts, dt) {
            n if n > 1 && !impact => swing(bodies, contacts, dt, n),
            _ => substep(bodies, contacts, impact, false),
        }
        for body in bodies.iter_mut() {
            body.crushing = false;
        }
    }
    // Members compressed enough to buckle are split, their new joints colliding as the
    // others do (`structure/buckling.rs`).
    for body in bodies.iter_mut() {
        for (node, radius) in body.solver.split_compressed(&mut body.structure) {
            body.shapes
                .push((node, super::shape::Shape::Sphere { radius }));
        }
    }
}

/// Of the energy that would yield a struck node's weakest member, the share an impact must
/// bring for its step to be resolved in impact substeps: a hundredth (a crash brings many
/// times it; a member landing on its next face, a millionth: a 12-sided pole rolling onto one
/// brings about 1 mJ to members storing 34 J at yield, and its 0.5 ms substeps made building
/// with poles ten times slower than with balls, G2.3).
const YIELDING_SHARE: f64 = 0.01;

/// Whether contact `c` is an impact that could make a structure's members at it flow: its
/// approach energy ½·m·v² at least [`YIELDING_SHARE`] of the least yielding energy at its
/// nodes (`Structure::yield_energy`). Rigid bodies need no substeps: the impact law is exact.
fn can_yield(bodies: &[&mut Body], c: &super::contact::Contact) -> bool {
    if !impact::is_impact(c) {
        return false;
    }
    // A face landing (a member's prism, G2.3) is judged by what it brings. A ball's impact
    // keeps its substeps as before: they also resolve a light joint's ringing against what it
    // strikes (the module docs, `crush_tests.rs`), and the saved fixtures hold that motion bit
    // for bit (`creeer_sim::save` tests).
    if !matches!(c.law, super::contact::Law::Patch { .. }) {
        return true;
    }
    let energy = 0.5 * c.mass * c.approach * c.approach;
    [c.a, c.b].into_iter().any(|side| match side {
        Side::Node { member, node, .. } => {
            let s = &bodies[member].structure;
            s.element_count() > 0 && energy >= YIELDING_SHARE * s.yield_energy(node)
        }
        Side::Fixed(_) => false,
    })
}

/// One step with the solvers' current time step. `resolved`: the step is short enough to
/// follow the impact's deceleration (impact substeps).
fn substep(bodies: &mut [&mut Body], contacts: &mut Contacts, impact: bool, resolved: bool) {
    // The contacts' laws as this step resolves them, before any energy is measured: no
    // contact carries more at rest than the weight of the whole group and its loads.
    let weight: f64 = bodies
        .iter()
        .map(|b| {
            let s = &b.structure;
            (0..s.node_count())
                .map(|i| (s.gravity * s.masses()[i] + s.load[i]).length())
                .sum::<f64>()
        })
        .sum();
    contacts.set_weight(weight);
    contacts.set_step(bodies[0].solver.time_step());
    // Turning normals from where this (sub)step starts, before the guard measures them.
    let states: Vec<MemberState<'_>> = bodies
        .iter()
        .map(|b| MemberState {
            position: &b.structure.position,
            orientation: &b.structure.orientation,
        })
        .collect();
    contacts.start_turning(&states);
    // The bodies' energy only: contacts found pressed into each other at the step's start hold
    // energy that never existed (a ball found inside a crate held 290 kJ, and the rebound was
    // allowed to give some of it back, F5).
    let before = if impact {
        bodies.iter().map(|b| b.structure.energy()).sum::<f64>()
    } else {
        0.0
    };
    if impact && !resolved {
        let h = bodies[0].solver.time_step();
        let gaps = measured_gaps(bodies, contacts);
        let mut structures: Vec<_> = bodies.iter_mut().map(|b| &mut b.structure).collect();
        impact::prestop(&mut structures, (&contacts.list, &gaps), h);
    }
    let held = bodies.iter().any(|b| b.held);
    let mut members: Vec<_> = bodies
        .iter_mut()
        .map(|b| (&mut b.structure, &mut b.solver))
        .collect();
    // A trapezoidal step reflects a velocity that a contact stops (v⁺ = 2·Δx/h − v⁻) and
    // never damps a contact's (unresolvable, kHz) rocking: impacts are stepped with backward
    // Euler (v⁺ = Δx/h, fully inelastic; the rebound comes from the impact law), other
    // contact groups with BDF2, which damps what the step cannot resolve and stays second
    // order for what it can (and is exact for the constant force of plastic flow). Free
    // bodies keep Newmark.
    let scheme = if impact {
        Some(Scheme::BackwardEuler)
    } else if contacts.list.is_empty()
        && !contacts.joints.limited()
        && !contacts.joints.has_struts()
        && !held
    {
        // (A joint's stops are stepped as contacts are, and struts and bodies held:
        // `Body::held`.)
        None
    } else {
        Some(Scheme::Bdf2)
    };
    step_group_with(&mut members, contacts, scheme);
    if impact {
        // The rebound may not lift the total energy above its level before the step, less
        // the plastic work done in the step (that energy is gone: it heated the metal).
        let kinetic: f64 = bodies.iter().map(|b| b.structure.node_energy().0).sum();
        let plastic: f64 = bodies.iter().map(|b| b.solver.last_yielding.work).sum();
        let budget = kinetic + before - group_energy(bodies, contacts) - plastic;
        let gaps = measured_gaps(bodies, contacts);
        let mut structures: Vec<_> = bodies.iter_mut().map(|b| &mut b.structure).collect();
        let kicked = impact::restitute(&mut structures, (&contacts.list, &gaps), budget);
        let capped = impact::cap(&mut structures, budget);
        // A 60 Hz step spreads the impact's deceleration over the whole step: carrying it
        // into the next Newmark step would push the body away again. Impact substeps follow
        // the deceleration and keep it, unless impulses changed the velocities.
        if !resolved || kicked || capped {
            for body in bodies.iter_mut() {
                body.solver.reset_history(&body.structure);
            }
        }
    }
}

/// A step split into impact substeps of at most [`IMPACT_STEP`]. Contacts lose their
/// dashpots: the substeps resolve their vibration, BDF2 damps what remains, and a dashpot
/// would take work from a joint pressed into the wall that a rigid wall never takes.
/// Contacts with a deforming structure get no restitution impulses: at this step its elastic rebound is resolved, and kicking
/// its light joints off the wall would only make them chatter. Contacts between rigid
/// bodies keep the impact law. The group keeps crushing while members flow faster than
/// [`FAST_FLOW`].
fn crush(bodies: &mut [&mut Body], contacts: &mut Contacts, dt: f64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a small count
    let n = (dt / IMPACT_STEP).ceil().max(1.0) as u32;
    crate::profile::add(crate::profile::Slot::ImpactSubsteps, u64::from(n));
    let h = dt / f64::from(n);
    for body in bodies.iter_mut() {
        body.solver.set_time_step(&body.structure, h);
    }
    let deformable = |side: Side| match side {
        Side::Node { member, .. } => bodies[member].structure.element_count() > 0,
        Side::Fixed(_) => false,
    };
    for c in &mut contacts.list {
        c.dashpot = false;
        if deformable(c.a) || deformable(c.b) {
            c.restitution = 0.0;
        }
    }
    let mut flowing = false;
    for k in 0..n {
        if k > 0 {
            refresh_approach(bodies, contacts);
        }
        // Backward Euler only where the impact law follows (rigid bodies): a deforming
        // structure's contacts are resolved by the substeps themselves.
        let impact = contacts
            .list
            .iter()
            .any(|c| c.restitution > 0.0 && impact::is_impact(c));
        substep(bodies, contacts, impact, true);
        flowing |= bodies.iter().any(|b| b.solver.last_yielding.flowing > 0);
    }
    // The crush goes on while a member flows as fast as a crash makes it: slower flow (a
    // structure sagging under the load that struck it) is followed by the 60 Hz step.
    let fast = bodies
        .iter()
        .flat_map(|b| &b.structure.elements)
        .map(|e| e.strain_of(&e.flow_rate))
        .fold(0.0, f64::max);
    for body in bodies.iter_mut() {
        body.solver.set_time_step(&body.structure, dt);
        body.crushing = flowing && fast > FAST_FLOW;
    }
}

/// The most a joint's two sides may turn relative to each other in one substep (rad).
/// Newmark's energy wavers by about (ω·h)²/4 on a joint's nonlinear swing, back and forth;
/// beyond the energy guard's slack (10⁻³) the guard took each rise back for a gain and redid
/// it dissipatively, and a double pendulum at 60 Hz lost a third of its energy in three
/// seconds (G1.1). At 0.05 rad a substep the wavering stays within it, as for boxes turning
/// (`turn.rs`).
const JOINT_TURN: f64 = 0.05;
/// Most substeps of a group whose joints turn fast.
const JOINT_SUBSTEPS: u32 = 8;

/// The substeps a group needs for its joints' relative turn in a step of `dt`.
fn joint_substeps(bodies: &[&mut Body], contacts: &Contacts, dt: f64) -> u32 {
    let spin = |side: Option<(usize, usize)>| {
        side.map_or(creeer_math::glam::DVec3::ZERO, |(m, n)| {
            bodies[m].structure.angular_velocity[n]
        })
    };
    let fastest = (contacts.joints.list.iter())
        .map(|(_, j)| (spin(Some(j.a)) - spin(j.b)).length())
        .fold(0.0, f64::max);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, clamped
    let n = (fastest * dt / JOINT_TURN).ceil() as u32;
    n.clamp(1, JOINT_SUBSTEPS)
}

/// A step in `n` substeps (a group whose joints turn fast: [`JOINT_TURN`]).
fn swing(bodies: &mut [&mut Body], contacts: &mut Contacts, dt: f64, n: u32) {
    let h = dt / f64::from(n);
    for body in bodies.iter_mut() {
        body.solver.set_time_step(&body.structure, h);
    }
    for k in 0..n {
        if k > 0 {
            refresh_approach(bodies, contacts);
        }
        substep(bodies, contacts, false, false);
    }
    for body in bodies.iter_mut() {
        body.solver.set_time_step(&body.structure, dt);
    }
}

/// Sets every contact's approach speed from the current velocities.
fn refresh_approach(bodies: &[&mut Body], contacts: &mut Contacts) {
    let velocity = |side: Side| match side {
        Side::Node {
            member,
            node,
            local,
        } => {
            let s = &bodies[member].structure;
            s.velocity[node] + s.angular_velocity[node].cross(s.orientation[node] * local)
        }
        Side::Fixed(_) => creeer_math::glam::DVec3::ZERO,
    };
    for c in &mut contacts.list {
        let relative = velocity(c.a) - velocity(c.b);
        c.approach = c.normal.dot(relative);
        c.sliding = (relative - c.normal * c.approach).length();
    }
}
