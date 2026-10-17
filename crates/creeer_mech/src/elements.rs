//! What each element does, in one place for the assembly, the books and the active set: a
//! rigid element's constraint row and its torques per unit multiplier, a sliding one's
//! torques.

use crate::drivetrain::{Element, Grip, HOUSING, Node};

/// Up to three nodes an element acts on, each with a coefficient (unused: the housing, 0).
pub(crate) type Terms = [(Node, f64); 3];

const NONE: (Node, f64) = (HOUSING, 0.0);

/// Which way power flows through a transmission (its losses applied that way).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Forward,
    Backward,
    /// Neither way consistently within a step: what it carries is about nothing, and so is
    /// its loss (lossless for the step: switching its efficiency either way contradicted the
    /// direction it then gave, round after round).
    Neutral,
}

/// A transmission's factor on its output's torque: efficiency forwards, 1/efficiency back.
fn through(efficiency: f64, flow: Flow) -> f64 {
    match flow {
        Flow::Forward => efficiency,
        Flow::Backward => 1.0 / efficiency,
        Flow::Neutral => 1.0,
    }
}

/// Whether element `e` is held rigid by a multiplier in grip state `grip`.
pub(crate) fn is_rigid(e: &Element, grip: Grip) -> bool {
    match *e {
        Element::Gear { backlash, .. } => backlash <= 0.0 || matches!(grip, Grip::Meshed(_)),
        Element::Planetary { .. } => true,
        Element::Belt { .. } | Element::Clutch { .. } => grip == Grip::Stuck,
        Element::Bearing { coulomb, .. } => coulomb > 0.0 && grip == Grip::Stuck,
        Element::Shaft { .. } => false,
    }
}

/// A rigid element's constraint, Σ row·ω = 0, and its torques per unit multiplier λ (the
/// torque it carries from its first node: −λ on that node).
pub(crate) fn rigid(e: &Element, forward: Flow) -> (Terms, Terms) {
    match *e {
        Element::Gear {
            a,
            b,
            ratio,
            efficiency,
            ..
        }
        | Element::Belt {
            a,
            b,
            ratio,
            efficiency,
            ..
        } => (
            [(b, 1.0), (a, -ratio), NONE],
            [(a, -1.0), (b, through(efficiency, forward) / ratio), NONE],
        ),
        Element::Planetary {
            sun,
            ring,
            carrier,
            teeth,
            efficiency,
        } => {
            // In the carrier's frame the sun drives the ring through the planets (forward) or
            // is driven by it; the ring's torque carries the loss.
            let ring_torque = teeth * through(efficiency, forward);
            (
                [(sun, 1.0), (ring, teeth), (carrier, -(1.0 + teeth))],
                [
                    (sun, -1.0),
                    (ring, -ring_torque),
                    (carrier, 1.0 + ring_torque),
                ],
            )
        }
        Element::Clutch { a, b, .. } => ([(a, 1.0), (b, -1.0), NONE], [(a, -1.0), (b, 1.0), NONE]),
        Element::Bearing { a, .. } => ([(a, 1.0), NONE, NONE], [(a, -1.0), NONE, NONE]),
        Element::Shaft { .. } => unreachable!("a shaft is compliant"),
    }
}

/// The torques of a friction element that slides (or a gear within its play): constant
/// over the step, against the slip.
pub(crate) fn sliding(e: &Element, grip: Grip, forward: Flow) -> Terms {
    let Grip::Slipping(s) = grip else {
        return [NONE; 3];
    };
    let s = f64::from(s);
    match *e {
        Element::Belt {
            a,
            b,
            ratio,
            efficiency,
            capacity,
        } => {
            let lambda = s * capacity;
            [
                (a, -lambda),
                (b, through(efficiency, forward) / ratio * lambda),
                NONE,
            ]
        }
        Element::Clutch { a, b, capacity } => [(a, -s * capacity), (b, s * capacity), NONE],
        Element::Bearing { a, coulomb, .. } => [(a, -s * coulomb), NONE, NONE],
        _ => [NONE; 3],
    }
}

/// A power through a transmission below this share of the drivetrain's torques times its
/// speeds is rounding (`grip::noise`): a differential going straight (its sun and carrier at
/// one speed), an idle alternator's belt, a stalled gear. Its direction is then not evidence:
/// taken as such, it flipped round after round (three a step, F6.4's bench), a stalled
/// motor's gear took a speed's rounding for running backwards, and either way its loss is
/// nothing.
pub(crate) const NOISE: f64 = 1e-10;

/// Where a transmission's power flows: forward when what it carries from its first node
/// (`carried`) drives it (for a planetary set, in its carrier's frame), `None` when that power
/// is at most `noise` (W). `speed` gives a node's speed (the step's midpoint: where its loss
/// is booked, so the loss it books is never negative).
pub(crate) fn flow(
    e: &Element,
    carried: f64,
    speed: impl Fn(Node) -> f64,
    noise: f64,
) -> Option<Flow> {
    let power = match *e {
        Element::Gear { a, .. } | Element::Belt { a, .. } => carried * speed(a),
        Element::Planetary { sun, carrier, .. } => carried * (speed(sun) - speed(carrier)),
        _ => return Some(Flow::Forward),
    };
    if power.abs() <= noise {
        None
    } else if power > 0.0 {
        Some(Flow::Forward)
    } else {
        Some(Flow::Backward)
    }
}
