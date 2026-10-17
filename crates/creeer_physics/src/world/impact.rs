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
///
/// The impulses carry Coulomb friction: each contact's tangential impulse stops its point
/// sliding, up to μ times its normal impulse. Without it a cube tumbling down a slope landed
/// on a frictionless impulse at its leading edge, which reversed its spin and left its
/// centre sliding at 3.5 m/s: the step after saw a resting load, too little friction to stop
/// it (F5).
/// `gaps`: the contacts' gaps as measured (`Contacts::measured_gaps`; a face's corner on the
/// ground is measured to the ground under the face, not along its normal from there).
pub fn prestop(
    structures: &mut [&mut Structure],
    (contacts, gaps): (&[Contact], &[f64]),
    h: f64,
) -> bool {
    let closing: Vec<(&Contact, f64)> = contacts
        .iter()
        .zip(gaps)
        .filter(|(c, _)| is_impact(c))
        .map(|(c, gap)| (c, -gap.max(0.0) / h))
        .collect();
    let mut total = vec![(0.0, DVec3::ZERO); closing.len()];
    for _ in 0..PRE_SWEEPS {
        for (k, (c, floor)) in closing.iter().enumerate() {
            let n = c.normal;
            let relative = |s: &[&mut Structure]| point_velocity(s, c.a) - point_velocity(s, c.b);
            let speed = n.dot(relative(structures));
            let w = mobility(structures, c, n);
            if w == 0.0 {
                continue;
            }
            let new_total = (total[k].0 + (floor - speed) / w).max(0.0);
            push(structures, c, n * (new_total - total[k].0));
            total[k].0 = new_total;
            total[k].1 = rub(structures, c, total[k]);
        }
    }
    total.iter().any(|t| t.0 > 0.0)
}

/// Friction with an impact: the tangential impulse that stops contact `c`'s point sliding,
/// within the cone of its accumulated normal impulse (`total`: normal, tangential so far).
/// Applies the change and returns the new accumulated tangential impulse. Each change
/// only slows the point's sliding, so it takes energy out.
fn rub(structures: &mut [&mut Structure], c: &Contact, (normal, so_far): (f64, DVec3)) -> DVec3 {
    let n = c.normal;
    let v = point_velocity(structures, c.a) - point_velocity(structures, c.b);
    let sliding = v - n * n.dot(v);
    let length = sliding.length();
    if length < 1e-12 {
        return so_far;
    }
    let t = sliding / length;
    let w = mobility(structures, c, t);
    if w == 0.0 {
        return so_far;
    }
    let wanted = so_far - t * (length / w);
    let limit = c.friction * normal;
    let new = if wanted.length() > limit {
        wanted * (limit / wanted.length())
    } else {
        wanted
    };
    push(structures, c, new - so_far);
    new
}

/// How fast contact `c`'s points part along `direction` per unit of impulse there (1/kg).
fn mobility(structures: &[&mut Structure], c: &Contact, direction: DVec3) -> f64 {
    let (a, b) = (
        side(structures, c.a, direction),
        side(structures, c.b, direction),
    );
    a.map_or(0.0, |s| s.4) + b.map_or(0.0, |s| s.4)
}

/// Applies `impulse` to contact `c`'s point on A, and its opposite on B.
fn push(structures: &mut [&mut Structure], c: &Contact, impulse: DVec3) {
    let length = impulse.length();
    if length == 0.0 {
        return;
    }
    let direction = impulse / length;
    for (side_of, sign) in [(c.a, 1.0), (c.b, -1.0)] {
        if let Some((member, node, _, turn, _)) = side(structures, side_of, direction) {
            let st = &mut structures[member];
            st.velocity[node] += direction * (sign * length / st.mass[node]);
            st.angular_velocity[node] += turn * (sign * length);
        }
    }
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
/// `gaps`: the contacts' gaps after the step, as measured.
pub fn restitute(
    structures: &mut [&mut Structure],
    (contacts, gaps): (&[Contact], &[f64]),
    budget: f64,
) -> bool {
    let impacts: Vec<&Contact> = contacts
        .iter()
        .zip(gaps)
        .filter(|(c, gap)| c.restitution > 0.0 && is_impact(c) && **gap < TOUCHING)
        .map(|(c, _)| c)
        .collect();
    if impacts.is_empty() {
        return false;
    }
    let before: Vec<(Vec<DVec3>, Vec<DVec3>)> = structures
        .iter()
        .map(|s| (s.velocity.clone(), s.angular_velocity.clone()))
        .collect();
    let mut total = vec![(0.0, DVec3::ZERO); impacts.len()];
    for _ in 0..SWEEPS {
        for (k, c) in impacts.iter().enumerate() {
            let n = c.normal;
            let speed = n.dot(point_velocity(structures, c.a) - point_velocity(structures, c.b));
            let target = -c.restitution * c.approach;
            let w = mobility(structures, c, n);
            if w == 0.0 {
                continue;
            }
            // Accumulated impulse stays ≥ 0: contacts push, never pull.
            let new_total = (total[k].0 + (target - speed) / w).max(0.0);
            push(structures, c, n * (new_total - total[k].0));
            total[k].0 = new_total;
            total[k].1 = rub(structures, c, total[k]);
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
