//! A character fallen (G1.2d): its body handed over to a ragdoll of fifteen segments on
//! fourteen joints, with the motion it had. The segments' masses are a person's fractions
//! (de Leva 1996, the suit's share spread alike, its backpack on the torso); the joints'
//! ranges a person's; the joints damped as muscle and ligament damp them.
//!
//! The handover keeps momentum and angular momentum: the ragdoll is placed with its centre
//! of mass where the body's was, moving at its velocity, turning at the spin that gives its own
//! inertia the body's angular momentum (`I_rag·ω' = I_body·ω`).

use super::CENTRE;
use crate::character;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::world::joint::{Joint, Kind};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};

/// A segment: its centre standing (above the feet, x across, y up, z forward; m), half
/// extents (m) and share of the mass.
pub(crate) type Segment = (DVec3, DVec3, f64);

const fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// Pelvis, torso (with the backpack), head; then each side's upper arm, forearm, hand, thigh,
/// shank, foot: first the side at −x (the right of a person facing +z with +y up, in a
/// right-handed frame), then the side at +x (the left).
///
/// Measured on the engineer (G1.3f, ADR-0026): Blender Studio's base mesh fitted to ANSUR II's
/// men of normal weight at 1.80 m, standing on boots' 3 cm soles, arms hanging (palms to the
/// thighs, so the forearms and hands hang a little forward). Each limb's box runs from joint to
/// joint; across, its section is the limb's measured section, an ellipse, as a box of the same
/// area (half widths √π/2 of the ellipse's). The trunk's and the feet's likewise from their
/// extents, the pelvis below the back joint and down to the gluteal fold, as de Leva segments
/// it. Every vertex of the trunk, head, hands and feet lies within 3 cm of its box; the
/// shoulders' and the hips' muscles bulge up to 4.5 cm past the upper arms' and thighs'
/// (ADR-0023, amended), where bigger boxes would meet the trunk's.
pub(crate) const SEGMENTS: [Segment; 15] = [
    (v(0.0, 0.986, 0.012), v(0.152, 0.127, 0.107), 0.130),
    (v(0.0, 1.343, 0.015), v(0.134, 0.229, 0.109), 0.305),
    (v(0.0, 1.682, 0.058), v(0.086, 0.149, 0.104), 0.069),
    (v(-0.195, 1.304, 0.0), v(0.054, 0.156, 0.084), 0.027),
    (v(-0.237, 1.024, 0.034), v(0.036, 0.131, 0.038), 0.016),
    (v(-0.258, 0.791, 0.126), v(0.032, 0.115, 0.066), 0.006),
    (v(-0.108, 0.742, 0.009), v(0.083, 0.219, 0.072), 0.142),
    (v(-0.122, 0.317, -0.058), v(0.052, 0.207, 0.055), 0.043),
    (v(-0.155, 0.065, 0.016), v(0.063, 0.065, 0.127), 0.014),
    (v(0.195, 1.304, 0.0), v(0.054, 0.156, 0.084), 0.027),
    (v(0.237, 1.024, 0.034), v(0.036, 0.131, 0.038), 0.016),
    (v(0.258, 0.791, 0.126), v(0.032, 0.115, 0.066), 0.006),
    (v(0.108, 0.742, 0.009), v(0.083, 0.219, 0.072), 0.142),
    (v(0.122, 0.317, -0.058), v(0.052, 0.207, 0.055), 0.043),
    (v(0.155, 0.065, 0.016), v(0.063, 0.065, 0.127), 0.014),
];

/// A joint: child and parent segment, where (standing, above the feet), its axis, and what
/// it lets turn (a ball's cone and twist, or a hinge's range; rad).
pub(crate) type Link = (usize, usize, DVec3, DVec3, Kind);

const fn ball(cone: f64, twist: f64) -> Kind {
    Kind::Ball {
        cone: Some(cone),
        twist: Some((-twist, twist)),
    }
}

const fn hinge(lo: f64, hi: f64) -> Kind {
    Kind::Hinge {
        range: Some((lo, hi)),
    }
}

/// The joints: back, neck; each side's shoulder, elbow, wrist, hip, knee, ankle. Where the
/// engineer has them (as the segments): the back at the waist (omphalion), the neck at the
/// seventh cervical vertebra's height, the limbs' at their joints' centres.
pub(crate) const LINKS: [Link; 14] = [
    (1, 0, v(0.0, 1.113, 0.0), DVec3::Y, ball(0.6, 0.5)),
    (2, 1, v(0.0, 1.56, 0.0), DVec3::Y, ball(0.7, 1.0)),
    (3, 1, v(-0.186, 1.459, -0.005), DVec3::NEG_Y, ball(1.6, 1.0)),
    (4, 3, v(-0.223, 1.148, 0.004), DVec3::X, hinge(0.0, 2.5)),
    (5, 4, v(-0.253, 0.899, 0.076), DVec3::X, hinge(-1.0, 1.0)),
    (6, 0, v(-0.085, 0.959, 0.0), DVec3::NEG_Y, ball(1.4, 0.6)),
    (7, 6, v(-0.125, 0.524, -0.038), DVec3::X, hinge(-2.4, 0.0)),
    (8, 7, v(-0.123, 0.111, -0.053), DVec3::X, hinge(-0.6, 0.6)),
    (9, 1, v(0.186, 1.459, -0.005), DVec3::NEG_Y, ball(1.6, 1.0)),
    (10, 9, v(0.223, 1.148, 0.004), DVec3::X, hinge(0.0, 2.5)),
    (11, 10, v(0.253, 0.899, 0.076), DVec3::X, hinge(-1.0, 1.0)),
    (12, 0, v(0.085, 0.959, 0.0), DVec3::NEG_Y, ball(1.4, 0.6)),
    (13, 12, v(0.125, 0.524, -0.038), DVec3::X, hinge(-2.4, 0.0)),
    (14, 13, v(0.123, 0.111, -0.053), DVec3::X, hinge(-0.6, 0.6)),
];

/// The hip joints' height standing: where legs pressed shorter are folded from. Above the
/// standing body's own hip (`character::HIP`): folding from below the joints left them pulled
/// apart from the thighs, and the crowd scene's worst tick took 385 ms to pull them back.
const HIPS: f64 = LINKS[5].2.y;

/// The joints' damping, as a damping ratio of the segment beyond each swinging about it as a
/// pendulum under standard gravity (a property of the body, not of the planet): a limp
/// person's limbs settle in a swing or two. One damping for all (2 N·m·s/rad) left a hanging
/// foot, a hundred times lighter in turning than a thigh, oozing for seconds, never asleep.
const DAMPING_RATIO: f64 = 0.5;

/// The damping (N·m·s/rad) of a joint at `at` carrying a segment of `mass` and `half` extents
/// whose centre is at `centre`: 2ζ·√(m·g₀·r·I), I about the joint.
fn damping(mass: f64, half: DVec3, centre: DVec3, at: DVec3) -> f64 {
    let r = (centre - at).length();
    let own = box_inertia(mass, half);
    let inertia = (own.x_axis.x + own.y_axis.y + own.z_axis.z) / 3.0 + mass * r * r;
    2.0 * DAMPING_RATIO * (mass * 9.806_65 * r * inertia).sqrt()
}
/// The segments' boots' and suit's friction.
const FRICTION: f64 = 0.8;
/// How far above the ground a segment found in it is put (m).
const LIFTED: f64 = 1e-3;

/// The head's segment.
pub const HEAD: usize = 2;

/// The loads that break a person's bones in a blow, about (N): upper arm, forearm, hand,
/// thigh, shank, foot (the femur's 8 kN against FMVSS 208's axial 10 kN; a wrist breaks in a
/// fall at 2–3 kN).
const BREAKS: [f64; 6] = [3000.0, 2500.0, 2000.0, 8000.0, 5000.0, 5000.0];

/// How an impact on segment `k` is judged (`injury::Region`).
pub fn region(k: usize) -> super::injury::Region {
    use super::injury::Region;
    match k {
        HEAD => Region::Head,
        0 | 1 => Region::Torso,
        _ => Region::Limb {
            breaks: BREAKS[(k - 3) % 6],
        },
    }
}

/// A box's inertia (kg·m²) of `mass` and `half` extents, in its frame.
fn box_inertia(mass: f64, half: DVec3) -> DMat3 {
    Shape::Box { half }.inertia(mass)
}

/// Character `index` of `world` falls: its body becomes the ragdoll's pelvis (the same index,
/// so whatever knows the character by it finds its pelvis) and the other fourteen segments
/// are added. Returns every segment's body index, the pelvis first.
pub fn fall(world: &mut World, index: usize) -> Vec<usize> {
    let old = &world.bodies[index];
    let s = &old.structure;
    let (x, q, v, w) = (
        s.position[0],
        s.orientation[0],
        s.velocity[0],
        s.angular_velocity[0],
    );
    let mass = s.masses()[0];
    let turn = DMat3::from_quat(q);
    let body_angular = turn * s.inertia_of(0) * turn.transpose() * w;
    let (gravity, planet) = (s.gravity, s.planet.clone());
    let dt = world.dt();
    // Each segment where it stands in the body's frame, its centre of mass on the body's. Legs
    // pressed shorter than they stand (the body down on its knees, or on its box) are folded
    // to the length they have: laid out standing, they were half a metre in the ground, and
    // its contacts threw them out (a fall of 3 m, ended at rest, then took two healths).
    let feet = x - q * DVec3::Y * CENTRE;
    let hip = feet + q * DVec3::Y * HIPS;
    let fold = (world.ray(hip, -(q * DVec3::Y), HIPS, Some(index)))
        .map_or(1.0, |h| (h.distance / HIPS).clamp(0.1, 1.0));
    let stand = |p: DVec3| {
        if p.y < HIPS {
            DVec3::new(p.x, HIPS - (HIPS - p.y) * fold, p.z)
        } else {
            p
        }
    };
    let masses: Vec<f64> = SEGMENTS.iter().map(|s| s.2 * mass).collect();
    let mut at: Vec<DVec3> = SEGMENTS.iter().map(|s| feet + q * stand(s.0)).collect();
    let centre = at.iter().zip(&masses).map(|(p, m)| *p * *m).sum::<DVec3>() / mass;
    for p in &mut at {
        *p += x - centre;
    }
    // Any still in the ground under the body (its arms beside a body lying on its side, feet
    // folded under it) out of it, along its normal.
    let down = planet.as_ref().map_or(gravity, |p| p.acceleration(x));
    if let Some(ground) = world.ray(x, down.normalize_or_zero(), 3.0, Some(index)) {
        let n = ground.normal;
        for (p, seg) in at.iter_mut().zip(&SEGMENTS) {
            let reach: f64 = (0..3)
                .map(|i| (n.dot(q * DVec3::AXES[i]) * seg.1[i]).abs())
                .sum();
            let gap = (*p - ground.point).dot(n) - reach;
            if gap < LIFTED {
                *p += n * (LIFTED - gap);
            }
        }
    }
    let at_of = at.clone();
    // The spin whose angular momentum, with the ragdoll's inertia, is the body's.
    let mut inertia = DMat3::ZERO;
    for ((p, m), seg) in at.iter().zip(&masses).zip(&SEGMENTS) {
        let r = *p - x;
        let own = turn * box_inertia(*m, seg.1) * turn.transpose();
        inertia += own + (DMat3::IDENTITY * r.length_squared() - outer(r, r)) * *m;
    }
    let spin = inertia.inverse() * body_angular;
    let mut parts = Vec::with_capacity(SEGMENTS.len());
    for (k, ((p, m), seg)) in at.iter().zip(&masses).zip(&SEGMENTS).enumerate() {
        let mut b = Body::rigid(Shape::Box { half: seg.1 }, *m, *p, dt);
        let st = &mut b.structure;
        st.orientation[0] = q;
        st.velocity[0] = v + spin.cross(*p - x);
        st.angular_velocity[0] = spin;
        b.friction = FRICTION;
        b.restitution = 0.0;
        if k == 0 {
            // The pelvis in the character's place, under its gravity.
            b.structure.gravity = gravity;
            b.structure.planet = planet.clone();
            b.solver.reset_history(&b.structure);
            world.bodies[index] = b;
            parts.push(index);
        } else {
            parts.push(world.add(b));
        }
    }
    for &(child, parent, at, axis, kind) in &LINKS {
        let at = feet + q * stand(at) + (x - centre);
        let axis = q * axis;
        let reference = axis.any_orthonormal_vector();
        let joint = Joint::new(
            world,
            ((parts[child], 0), Some((parts[parent], 0))),
            (at, axis, reference),
            kind,
        );
        let j = world.join(joint);
        let (m, half) = (SEGMENTS[child].2 * mass, SEGMENTS[child].1);
        world.joint_mut(j).damping = damping(m, half, at_of[child], at);
    }
    pass_within(world, &parts);
    parts
}

/// The arms: each side's upper arm, forearm and hand.
const ARMS: [usize; 6] = [3, 4, 5, 9, 10, 11];
/// The pelvis, the torso and the head.
const CORE: [usize; 3] = [0, 1, HEAD];

/// Lets a ragdoll's segments (`parts`, the pelvis first) pass through each other but for the
/// arms against the pelvis, torso and head: so an arm cannot pass through the chest, while
/// the limbs' other contacts with each other, as close as a person's limbs hang, kept fallen
/// ragdolls restless (the contacts found anew each step: G1.8; ADR-0023, amended in G1.3f).
/// Whoever loads a world with fallen people calls it again: the pairs are not saved.
pub fn pass_within(world: &mut World, parts: &[usize]) {
    for (a, &x) in parts.iter().enumerate() {
        for (b, &y) in parts.iter().enumerate().skip(a + 1) {
            let arm_on_core = (ARMS.contains(&a) && CORE.contains(&b))
                || (CORE.contains(&a) && ARMS.contains(&b));
            if !arm_on_core {
                world.let_pass(x, y);
            }
        }
    }
}

/// How far a character may lean before it has toppled (cosine of 45°).
const TOPPLED: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// The velocity change in a step what it meets may give it before it is knocked down (m/s):
/// people keep their feet by stepping after pushes that move their centre at 1–1.5 m/s, not
/// after one of 2 (walking into a wall is not one; running into it, or landing on the hips
/// from 3 m, is).
const STRUCK: f64 = 2.0;

/// Whether character `index` was knocked down in the last step (moving at `before` when it
/// began, its legs pushing it with `pushed`): toppled (leaning past 45° from `up`), or struck (jolted by more than [`STRUCK`]).
/// The jolt, not its contacts' forces: an impact's impulse is the impact law's, outside them.
pub fn knocked(world: &World, index: usize, up: DVec3, motion: (DVec3, DVec3)) -> bool {
    let s = &world.bodies[index].structure;
    if (s.orientation[0] * DVec3::Y).dot(up) < TOPPLED {
        return true;
    }
    super::injury::jolt(world, index, motion).length() > STRUCK
}

fn outer(a: DVec3, b: DVec3) -> DMat3 {
    DMat3::from_cols(a * b.x, a * b.y, a * b.z)
}

/// The character standing again where its fallen pelvis lies (`parts` its segments, the
/// pelvis first): its body, upright on `up`, its feet at the ground under the pelvis
/// (`feet`), at rest, with the ragdoll's mass. Whoever holds the parts removes the others.
pub fn standing(world: &World, parts: &[usize], feet: DVec3, up: DVec3) -> Body {
    let mass: f64 = (parts.iter())
        .map(|&p| world.bodies[p].structure.masses()[0])
        .sum();
    let pelvis = &world.bodies[parts[0]].structure;
    let mut body = character::body(feet, up, world.dt());
    body.structure.gravity = pelvis.gravity;
    body.structure.planet = pelvis.planet.clone();
    body.solver.set_mass(&mut body.structure, 0, mass);
    body.solver.reset_history(&body.structure);
    body
}

#[cfg(test)]
#[path = "ragdoll_tests.rs"]
mod tests;
