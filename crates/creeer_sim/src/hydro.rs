//! Turbines on drivetrains (F6.8, ADR-0016): the water's torque on a runner every tick, from
//! the turbine's curve at the flow its pipe network last found and the shaft's speed, however
//! seldom that network steps.
//!
//! **Exact on the shaft.** The torque is linear in the shaft's speed, T = T₀ + s·ω
//! (`Network::turbine_curve`), and the drivetrain's step is linear in the torque on the runner,
//! ω₁ = free + mobility·T (`Drivetrain::response`). The torque is solved at the step's midpoint
//! speed, T = T₀ + s·(ω₀ + ω₁)/2, which is the speed the drivetrain books its work at: the work
//! taken out of the water (`Network::turbine_work`) is, joule for joule, the work the
//! drivetrain books as imported. A second machine on the same drivetrain sees the turbine's
//! torque as it was set (one coupling a step behind the other, as `drive.rs` does for two
//! machines).
//!
//! **A slow network.** Between its steps the flow is the one it last found, the shaft's speed
//! each tick's own: the torque follows the shaft at once, and the network's next step starts
//! from the speed the shaft has then.

use creeer_fluids::pipes::Network as Pipes;
use creeer_mech::{Drivetrain, Node};

/// A turbine (a link of a pipe network) driving a drivetrain's part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurbineLink {
    pub turbine: usize,
    /// The drivetrain (index) and its part the runner is.
    pub drivetrain: usize,
    pub node: Node,
}

/// Before the drivetrains step by `h`: each turbine's torque on its runner, at the step's
/// midpoint speed. Returns the runners' speeds now, for [`finish`].
pub fn prepare(
    pipes: &Pipes,
    drivetrains: &mut [Drivetrain],
    links: &[TurbineLink],
    h: f64,
) -> Vec<f64> {
    links
        .iter()
        .map(|link| {
            let d = &mut drivetrains[link.drivetrain];
            let (at_rest, slope) = pipes.turbine_curve(link.turbine);
            d.set_torque(link.node, 0.0);
            let (free, mobility) = d.response(link.node, h);
            let start = d.speed(link.node);
            // T = T₀ + s·(ω₀ + free + mobility·T)/2.
            let torque = (at_rest + 0.5 * slope * (start + free)) / (1.0 - 0.5 * slope * mobility);
            d.set_torque(link.node, torque);
            start
        })
        .collect()
}

/// After the drivetrains' step of `h`: each turbine's work, as its drivetrain booked it, taken
/// out of its water; and its runner's speed for its network's next step. Returns the works (J).
pub fn finish(
    pipes: &mut Pipes,
    drivetrains: &[Drivetrain],
    links: &[TurbineLink],
    started: &[f64],
    h: f64,
) -> Vec<f64> {
    links
        .iter()
        .zip(started)
        .map(|(link, &start)| {
            let d = &drivetrains[link.drivetrain];
            let speed = d.speed(link.node);
            let work = d.torque(link.node) * 0.5 * (start + speed) * h;
            pipes.turbine_work(link.turbine, work);
            pipes.set_turbine_speed(link.turbine, speed);
            work
        })
        .collect()
}
