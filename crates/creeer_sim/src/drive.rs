//! Electric machines on drivetrains (F6.4, ADR-0012): a DC machine's rotor is a part of a
//! drivetrain, and each step its circuit and its drivetrain are solved as one.
//!
//! The drivetrain's step is linear in the torque on a part (for the friction elements'
//! states it starts with): the part ends at ω = free + mobility·T. The machine's circuit
//! takes that as its rotor's equation (`Circuit::set_shaft`), so its armature current,
//! back-EMF and torque are solved with the drivetrain's response; the drivetrain then steps
//! with that torque and ends where the circuit said. A motor stalling under a load is the
//! same stall on both sides, and the work the circuit exports is the work the drivetrain
//! imports, joule for joule.
//!
//! **Where it is not exact.** A friction element that switches within the drivetrain's step
//! (a clutch locking, a brake letting go) changes the response the circuit was given: the
//! two then differ for that one step, by what the switch changed (each ledger still closes
//! on its own; the next step starts from the drivetrain's speed). Two machines on one
//! drivetrain each see their own part's response with the other's torque from the step
//! before: the coupling between them lags one step.

use creeer_electric::{Circuit, ComponentId, Shaft};
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, MechReport, Node};
use rayon::prelude::*;

/// A machine whose rotor is a drivetrain's part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MachineLink {
    /// The machine in the circuit.
    pub machine: ComponentId,
    /// The drivetrain (index) and its part the rotor is.
    pub drivetrain: usize,
    pub node: Node,
}

/// Advances `circuit` and `drivetrains` by `h` seconds together, the machines of `links` on
/// their drivetrains, each drivetrain's housing turning at its `spin` (rad/s, its frame).
///
/// # Panics
/// If `spin` does not give every drivetrain a spin.
pub fn step(
    circuit: &mut Circuit,
    drivetrains: &mut [Drivetrain],
    links: &[MachineLink],
    h: f64,
    spin: &[DVec3],
) -> Vec<MechReport> {
    assert_eq!(spin.len(), drivetrains.len(), "a spin for each drivetrain");
    prepare(circuit, drivetrains, links, h);
    circuit.step(h);
    finish(circuit, drivetrains, links, h, spin)
}

/// [`step`]'s first half: each machine's rotor's response handed to the circuit, for a circuit
/// that other couplings step too (its own step between this and [`finish`]).
pub fn prepare(
    circuit: &mut Circuit,
    drivetrains: &mut [Drivetrain],
    links: &[MachineLink],
    h: f64,
) {
    for link in links {
        let d = &mut drivetrains[link.drivetrain];
        // The response without the machine's own torque (the circuit adds it), the others'
        // as they were.
        let own = d.torque(link.node);
        d.set_torque(link.node, 0.0);
        let (free, mobility) = d.response(link.node, h);
        circuit.set_shaft(
            link.machine,
            Some(Shaft {
                speed: d.speed(link.node),
                free,
                mobility,
            }),
        );
        d.set_torque(link.node, own);
    }
}

/// [`step`]'s second half, after the circuit's step: the machines' torques on their rotors, and
/// the drivetrains' steps.
pub fn finish(
    circuit: &Circuit,
    drivetrains: &mut [Drivetrain],
    links: &[MachineLink],
    h: f64,
    spin: &[DVec3],
) -> Vec<MechReport> {
    for link in links {
        drivetrains[link.drivetrain].set_torque(link.node, circuit.torque(link.machine));
    }
    drivetrains
        .par_iter_mut()
        .zip(spin)
        .map(|(d, &s)| d.step(h, s))
        .collect()
}
