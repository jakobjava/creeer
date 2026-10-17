//! The active set: after a round's solve, which elements must switch (a stuck friction
//! element asked for more than it holds slips, a slipping one whose slip reversed sticks, a
//! meshed flank pulled apart lets go, a gear crossing its play meshes) and which
//! transmissions' power changed direction (their efficiency applies the other way).

use crate::drivetrain::{Drivetrain, Element, Grip, HOUSING};
use crate::elements::{self, Flow};
use crate::step::speed;

fn sign(v: f64) -> i8 {
    if v >= 0.0 { 1 } else { -1 }
}

/// The drivetrain's torque scale (N·m): the largest of the torques on its parts and of
/// `carried`, what its elements carry.
pub(crate) fn scale(d: &Drivetrain, carried: &[f64]) -> f64 {
    let parts = d.parts.iter().map(|p| p.torque.abs() + p.load.abs());
    carried
        .iter()
        .map(|c| c.abs())
        .chain(parts)
        .fold(0.0, f64::max)
}

/// The power through a transmission that is rounding (W, `elements::NOISE`): against the
/// drivetrain's torques and its speeds, the fastest part's or what a step's torque makes of
/// the lightest (at rest, a speed's rounding is of that).
pub(crate) fn noise(d: &Drivetrain, carried: &[f64], h: f64) -> f64 {
    let torques = scale(d, carried);
    let parts = d.parts.iter().skip(1);
    let fastest = parts.clone().map(|p| p.speed.abs()).fold(0.0, f64::max);
    let lightest = parts.map(|p| p.inertia).fold(f64::INFINITY, f64::min);
    elements::NOISE * torques * (fastest + h * torques / lightest)
}

/// Whether a slip `slip` of a friction element slipping in direction `s` reversed: by more than
/// a billionth of what its own friction `capacity` changes it in a step of `h` (and than the
/// speeds' rounding). Only reaching zero is not reversing: an element held still by another
/// (a bearing on a braked shaft) slips at its capacity with zero slip, saturated; were that
/// taken as sticking, the split between the two would flip it back and forth.
fn reversed(d: &Drivetrain, slip: f64, s: i8, h: f64, capacity: f64, nodes: [usize; 2]) -> bool {
    let mobility: f64 = nodes
        .iter()
        .filter(|&&n| n != HOUSING)
        .map(|&n| 1.0 / d.parts[n].inertia)
        .sum();
    let speeds: f64 = nodes.iter().map(|&n| d.parts[n].speed.abs()).sum();
    slip * f64::from(s) < -(1e-9 * h * capacity * mobility + 1e-14 * speeds)
}

/// Switches what the round's solution `x` (of a step of `h`) contradicts; returns whether
/// anything switched.
pub(crate) fn update(
    d: &Drivetrain,
    h: f64,
    x: &[f64],
    column: &[Option<usize>],
    grip: &mut [Grip],
    forward: &mut [Flow],
    flips: &mut [u8],
) -> bool {
    let mut changed = false;
    let noise = noise(d, &x[d.parts.len() - 1..], h);
    // Power is judged at the step's midpoint, where its loss is booked.
    let mid = |node: usize| {
        if node == HOUSING {
            0.0
        } else {
            0.5 * (x[node - 1] + d.parts[node].speed)
        }
    };
    for (k, e) in d.elements.iter().enumerate() {
        let lambda = column[k].map(|c| x[c]);
        let next = match *e {
            Element::Shaft { .. } | Element::Planetary { .. } => grip[k],
            Element::Gear {
                a,
                b,
                ratio,
                backlash,
                ..
            } => {
                if backlash <= 0.0 {
                    grip[k]
                } else {
                    // A flank pushes only: on the positive one (b ahead) its torque on b is
                    // negative, λ·ratio ≤ 0; on the negative one the opposite.
                    let half = 0.5 * backlash;
                    match (grip[k], lambda) {
                        (Grip::Meshed(f), Some(l)) if f64::from(f) * l * ratio > 0.0 => Grip::Free,
                        (Grip::Meshed(f), _) => Grip::Meshed(f),
                        (_, _) => {
                            let place = d.twist[k] + h * (speed(x, b) - ratio * speed(x, a));
                            if place > half {
                                Grip::Meshed(1)
                            } else if place < -half {
                                Grip::Meshed(-1)
                            } else {
                                Grip::Free
                            }
                        }
                    }
                }
            }
            Element::Belt {
                a,
                b,
                ratio,
                capacity,
                ..
            } => {
                // Positive when `a` runs ahead of what `b` turns at (in `a`'s terms: a crossed
                // belt's negative ratio turns `b` the other way).
                let slip = speed(x, a) - speed(x, b) / ratio;
                match (grip[k], lambda) {
                    (Grip::Stuck, Some(l)) if l.abs() > capacity => Grip::Slipping(sign(l)),
                    (Grip::Slipping(s), _) if reversed(d, slip, s, h, capacity, [a, b]) => {
                        Grip::Stuck
                    }
                    (g, _) => g,
                }
            }
            Element::Clutch { a, b, capacity } => {
                let slip = speed(x, a) - speed(x, b);
                match (grip[k], lambda) {
                    // Released: it slips whatever the speeds, carrying nothing (which way does not
                    // matter: switching that every round as a slip near zero changes sign would
                    // never settle).
                    (Grip::Slipping(s), _) if capacity <= 0.0 => Grip::Slipping(s),
                    _ if capacity <= 0.0 => Grip::Slipping(sign(slip)),
                    (Grip::Stuck, Some(l)) if l.abs() > capacity => Grip::Slipping(sign(l)),
                    (Grip::Slipping(s), _) if reversed(d, slip, s, h, capacity, [a, b]) => {
                        Grip::Stuck
                    }
                    (g, _) => g,
                }
            }
            Element::Bearing { a, coulomb, .. } => {
                if coulomb <= 0.0 {
                    grip[k]
                } else {
                    match (grip[k], lambda) {
                        (Grip::Stuck, Some(l)) if l.abs() > coulomb => Grip::Slipping(sign(l)),
                        (Grip::Slipping(s), _)
                            if reversed(d, speed(x, a), s, h, coulomb, [a, HOUSING]) =>
                        {
                            Grip::Stuck
                        }
                        (g, _) => g,
                    }
                }
            }
        };
        if next != grip[k] {
            changed = true;
            grip[k] = next;
        }
    }
    if changed {
        // A round whose grips were wrong (a brake slipping the wrong way) is no evidence of
        // where power flows: taken as such, a stalling motor's gear flipped and went neutral.
        return true;
    }
    for (k, e) in d.elements.iter().enumerate() {
        // What it carries from its first node at this solution, and so where power flows
        // (kept as it was once neutral, and where its power is rounding).
        let carried = match (
            column[k].map(|c| x[c]),
            elements::sliding(e, grip[k], forward[k]),
        ) {
            (Some(l), _) => l,
            (None, terms) => -terms[0].1,
        };
        let mut flows = match elements::flow(e, carried, mid, noise) {
            Some(f) if forward[k] != Flow::Neutral => f,
            _ => forward[k],
        };
        if flows != forward[k] {
            // Twice reversed within the step: it carries about nothing either way (`Flow`).
            flips[k] += 1;
            if flips[k] >= 2 {
                flows = Flow::Neutral;
            }
            changed = true;
            forward[k] = flows;
        }
    }
    changed
}
