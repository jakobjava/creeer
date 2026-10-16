//! Newton's impact law, applied after the step: a contact that was approaching at speed
//! v⁻ and is now touching leaves with at least −e·v⁻ (e: restitution).
//!
//! The contact's own vibration lasts about a millisecond and cannot be resolved by a 60 Hz
//! step, so the step itself absorbs it (fully inelastic). The rebound is then given back
//! here as impulses, solved by sequential impulses over all contacts of a group: each
//! impulse pushes only (never pulls), so impacts never add energy.

use super::contact::{Contact, Side};
use crate::structure::Structure;
use creeer_math::glam::{DMat3, DVec3};

/// Below this approach speed (m/s) a contact is resting, not impacting.
const RESTING_SPEED: f64 = 0.05;
/// A contact counts as touching after the step if its gap is below this (m).
const TOUCHING: f64 = 1.0e-4;
/// Sequential-impulse sweeps.
const SWEEPS: usize = 16;

/// Velocity of a side's contact point, its lever arm, and its inverse mass along `n`.
fn side(
    structures: &[&mut Structure],
    side: Side,
    n: DVec3,
) -> Option<(usize, usize, DVec3, DVec3, f64)> {
    let Side::Node {
        member,
        node,
        local,
    } = side
    else {
        return None;
    };
    let s = &structures[member];
    let arm = s.orientation[node] * local;
    let r = DMat3::from_quat(s.orientation[node]);
    let inverse_inertia = (r * s.inertia[node] * r.transpose()).inverse();
    let rn = arm.cross(n);
    let w = 1.0 / s.mass[node] + rn.dot(inverse_inertia * rn);
    Some((member, node, arm, inverse_inertia * rn, w))
}

fn point_velocity(structures: &[&mut Structure], side: Side) -> DVec3 {
    match side {
        Side::Node {
            member,
            node,
            local,
        } => {
            let s = &structures[member];
            s.velocity[node] + s.angular_velocity[node].cross(s.orientation[node] * local)
        }
        Side::Fixed(_) => DVec3::ZERO,
    }
}

fn gap(structures: &[&mut Structure], c: &Contact) -> f64 {
    let at = |side: Side| match side {
        Side::Node {
            member,
            node,
            local,
        } => {
            let s = &structures[member];
            s.position[node] + s.orientation[node] * local
        }
        Side::Fixed(p) => p,
    };
    c.normal.dot(at(c.a) - at(c.b)) - c.offset
}

/// Whether the contact is closing fast enough to be an impact.
pub fn is_impact(c: &Contact) -> bool {
    c.approach < -RESTING_SPEED
}

/// Sweeps of the impulses before an impact step: a push crosses one contact per sweep, so a
/// stack of n bodies needs about n sweeps.
const PRE_SWEEPS: usize = 32;

/// The inelastic part of the impacts, before the step (speculative contacts): every
/// contact that closes is slowed by impulses (which only push) until it closes no faster
/// than it can within the step, −gap/h. The implicit step then starts from velocities that
/// do not drive bodies deep into each other: its prediction x + h·v would otherwise put a
/// ball hitting a stack 20 cm inside a crate, and the minimisation needed hundreds of
/// iterations to push it out. Impulses that only push can only take energy out; the
/// bounce still follows from the impact law after the step. Returns whether any acted.
pub fn prestop(structures: &mut [&mut Structure], contacts: &[Contact], h: f64) -> bool {
    let closing: Vec<(&Contact, f64)> = contacts
        .iter()
        .filter(|c| is_impact(c))
        .map(|c| (c, -gap(structures, c).max(0.0) / h))
        .collect();
    let mut total = vec![0.0; closing.len()];
    for _ in 0..PRE_SWEEPS {
        for (k, (c, floor)) in closing.iter().enumerate() {
            let n = c.normal;
            let speed = n.dot(point_velocity(structures, c.a) - point_velocity(structures, c.b));
            let (a, b) = (side(structures, c.a, n), side(structures, c.b, n));
            let w = a.map_or(0.0, |s| s.4) + b.map_or(0.0, |s| s.4);
            if w == 0.0 {
                continue;
            }
            let new_total = (total[k] + (floor - speed) / w).max(0.0);
            let impulse = new_total - total[k];
            total[k] = new_total;
            for (s, sign) in [(a, 1.0), (b, -1.0)] {
                if let Some((member, node, _, turn, _)) = s {
                    let st = &mut structures[member];
                    st.velocity[node] += n * (sign * impulse / st.mass[node]);
                    st.angular_velocity[node] += turn * (sign * impulse);
                }
            }
        }
    }
    total.iter().any(|t| *t > 0.0)
}

/// Kinetic energy of the structures (J).
fn kinetic(structures: &[&mut Structure]) -> f64 {
    structures.iter().map(|s| s.node_energy().0).sum()
}

/// Applies the impact law to the contacts of a group after its step. The kinetic energy
/// afterwards may not exceed `budget`: with several contacts at once (a box landing on a
/// corner and an edge), sequential impulses can overshoot, so the impulses are scaled back
/// to the largest fraction s ≤ 1 within the budget. Kinetic energy is quadratic in s, so
/// that fraction is exact. Returns whether any impulse was applied.
pub fn restitute(structures: &mut [&mut Structure], contacts: &[Contact], budget: f64) -> bool {
    let impacts: Vec<&Contact> = contacts
        .iter()
        .filter(|c| c.restitution > 0.0 && is_impact(c) && gap(structures, c) < TOUCHING)
        .collect();
    if impacts.is_empty() {
        return false;
    }
    let before: Vec<(Vec<DVec3>, Vec<DVec3>)> = structures
        .iter()
        .map(|s| (s.velocity.clone(), s.angular_velocity.clone()))
        .collect();
    let mut total = vec![0.0; impacts.len()];
    for _ in 0..SWEEPS {
        for (k, c) in impacts.iter().enumerate() {
            let n = c.normal;
            let speed = n.dot(point_velocity(structures, c.a) - point_velocity(structures, c.b));
            let target = -c.restitution * c.approach;
            let (a, b) = (side(structures, c.a, n), side(structures, c.b, n));
            let w = a.map_or(0.0, |s| s.4) + b.map_or(0.0, |s| s.4);
            if w == 0.0 {
                continue;
            }
            // Accumulated impulse stays ≥ 0: contacts push, never pull.
            let new_total = (total[k] + (target - speed) / w).max(0.0);
            let impulse = new_total - total[k];
            total[k] = new_total;
            for (s, sign) in [(a, 1.0), (b, -1.0)] {
                if let Some((member, node, _, turn, _)) = s {
                    let st = &mut structures[member];
                    st.velocity[node] += n * (sign * impulse / st.mass[node]);
                    st.angular_velocity[node] += turn * (sign * impulse);
                }
            }
        }
    }
    limit(structures, &before, budget);
    true
}

/// Scales the velocity change since `before` by the largest s ∈ [0, 1] that keeps the
/// kinetic energy within `budget`.
fn limit(structures: &mut [&mut Structure], before: &[(Vec<DVec3>, Vec<DVec3>)], budget: f64) {
    let full = kinetic(structures);
    if full <= budget {
        return;
    }
    let set = |structures: &mut [&mut Structure], s: f64| {
        for (st, (v0, w0)) in structures.iter_mut().zip(before) {
            for i in 0..st.node_count() {
                let (v1, w1) = (st.velocity[i], st.angular_velocity[i]);
                st.velocity[i] = v0[i] + (v1 - v0[i]) * s;
                st.angular_velocity[i] = w0[i] + (w1 - w0[i]) * s;
            }
        }
    };
    // K(s) = c + b·s + a·s² from K(0), K(½), K(1).
    let k1 = full;
    let snapshot: Vec<(Vec<DVec3>, Vec<DVec3>)> = structures
        .iter()
        .map(|s| (s.velocity.clone(), s.angular_velocity.clone()))
        .collect();
    let restore = |structures: &mut [&mut Structure]| {
        for (st, (v, w)) in structures.iter_mut().zip(&snapshot) {
            st.velocity.clone_from(v);
            st.angular_velocity.clone_from(w);
        }
    };
    set(structures, 0.0);
    let k0 = kinetic(structures);
    restore(structures);
    set(structures, 0.5);
    let kh = kinetic(structures);
    restore(structures);
    let a = 2.0 * k1 - 4.0 * kh + 2.0 * k0;
    let b = k1 - k0 - a;
    // Largest root of a·s² + b·s + (k0 − budget) = 0 in [0, 1].
    let c = k0 - budget;
    let s = if c >= 0.0 {
        0.0
    } else if a.abs() < 1e-12 {
        (-c / b).clamp(0.0, 1.0)
    } else {
        ((-b + (b * b - 4.0 * a * c).max(0.0).sqrt()) / (2.0 * a)).clamp(0.0, 1.0)
    };
    set(structures, s);
}

/// Last resort after an impact step: if the kinetic energy still exceeds `budget` (the
/// step itself gained energy, which a backward Euler step through a corner contact can, by
/// a few percent), all velocities of the group are scaled back to the budget. Returns
/// whether it did.
pub fn cap(structures: &mut [&mut Structure], budget: f64) -> bool {
    let k = kinetic(structures);
    if k <= budget || k <= 0.0 {
        return false;
    }
    let factor = (budget.max(0.0) / k).sqrt();
    for s in structures.iter_mut() {
        for v in s.velocity.iter_mut().chain(s.angular_velocity.iter_mut()) {
            *v *= factor;
        }
    }
    true
}
