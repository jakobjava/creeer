//! The active set: after a round's solve, which elements must switch (a stuck friction
//! element asked for more than it holds slips, a slipping one whose slip reversed sticks, a
//! meshed flank pulled apart lets go, a gear crossing its play meshes) and which
//! transmissions' power changed direction (their efficiency applies the other way).

use crate::drivetrain::{Drivetrain, Element, Grip, HOUSING};
use crate::elements;
use crate::step::speed;

fn sign(v: f64) -> i8 {
    if v >= 0.0 { 1 } else { -1 }
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
    forward: &mut [bool],
) -> bool {
    let mut changed = false;
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
                    // Released: it slips whatever the speeds, carrying nothing.
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
        // What it carries from its first node at this solution, and so where power flows.
        let carried = match (lambda, elements::sliding(e, next, forward[k])) {
            (Some(l), _) => l,
            (None, terms) => -terms[0].1,
        };
        let flows = if next == grip[k] {
            elements::flows_forward(e, carried, |node| speed(x, node))
        } else {
            forward[k]
        };
        if next != grip[k] || flows != forward[k] {
            changed = true;
            grip[k] = next;
            forward[k] = flows;
        }
    }
    changed
}
