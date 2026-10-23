//! The jetpack's step (G1.2c, ADR-0023): its push, its attitude jets, the gas they spend.

use super::{ATTITUDE, G0, ISP, THRUST_MOST, gas};
use creeer_math::glam::DVec3;
use creeer_physics::world::World;

/// The jetpack for a step: its push (`thrust` as a fraction of its most, while gas lasts)
/// and its attitude jets' hold on the spin `w` (its body carrying `load` kg besides), and the body's mass after the step. The gas
/// they spend is the push over its exhaust's speed (Isp·g₀); the body steps with half the
/// step's gas spent (the mean mass over the step: Tsiolkovsky's Δv to second order) and ends
/// it without the step's.
pub(super) fn jetpack(
    world: &mut World,
    index: usize,
    thrust: DVec3,
    w: DVec3,
    load: f64,
) -> Option<(DVec3, DVec3, f64)> {
    let body = &mut world.bodies[index];
    let (mass, gas) = (body.structure.masses()[0], gas(body, load));
    if thrust == DVec3::ZERO || gas <= 0.0 {
        return None;
    }
    let exhaust = ISP * G0;
    let (damping, most, arm) = ATTITUDE;
    let mut push = thrust.clamp_length_max(1.0) * THRUST_MOST;
    let mut hold = (-w * damping).clamp_length_max(most);
    // Kilograms a step: the push's, and the attitude jets' (a pair at their lever).
    let dt = world.dt();
    let mut burn = (push.length() + hold.length() / arm) / exhaust * dt;
    if burn > gas {
        (push, hold) = (push * (gas / burn), hold * (gas / burn));
        burn = gas;
    }
    let body = &mut world.bodies[index];
    body.solver
        .set_mass(&mut body.structure, 0, mass - 0.5 * burn);
    Some((push, hold, mass - burn))
}
