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
type Segment = (DVec3, DVec3, f64);

const fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// Pelvis, torso (with the backpack), head; then each side's upper arm, forearm, hand, thigh,
/// shank, foot (left, then right).
const SEGMENTS: [Segment; 15] = [
    (v(0.0, 1.0, 0.0), v(0.16, 0.1, 0.11), 0.130),
    (v(0.0, 1.35, -0.02), v(0.19, 0.25, 0.14), 0.305),
    (v(0.0, 1.73, 0.0), v(0.11, 0.13, 0.12), 0.069),
    (v(-0.28, 1.43, 0.0), v(0.055, 0.15, 0.055), 0.027),
    (v(-0.28, 1.13, 0.0), v(0.045, 0.14, 0.045), 0.016),
    (v(-0.28, 0.93, 0.0), v(0.045, 0.05, 0.025), 0.006),
    (v(-0.1, 0.7, 0.0), v(0.075, 0.2, 0.075), 0.142),
    (v(-0.1, 0.3, 0.0), v(0.055, 0.19, 0.055), 0.043),
    (v(-0.1, 0.05, 0.05), v(0.055, 0.04, 0.13), 0.014),
    (v(0.28, 1.43, 0.0), v(0.055, 0.15, 0.055), 0.027),
    (v(0.28, 1.13, 0.0), v(0.045, 0.14, 0.045), 0.016),
    (v(0.28, 0.93, 0.0), v(0.045, 0.05, 0.025), 0.006),
    (v(0.1, 0.7, 0.0), v(0.075, 0.2, 0.075), 0.142),
    (v(0.1, 0.3, 0.0), v(0.055, 0.19, 0.055), 0.043),
    (v(0.1, 0.05, 0.05), v(0.055, 0.04, 0.13), 0.014),
];

/// A joint: child and parent segment, where (standing, above the feet), its axis, and what
/// it lets turn (a ball's cone and twist, or a hinge's range; rad).
type Link = (usize, usize, DVec3, DVec3, Kind);

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

/// The joints: back, neck; each side's shoulder, elbow, wrist, hip, knee, ankle.
const LINKS: [Link; 14] = [
    (1, 0, v(0.0, 1.1, 0.0), DVec3::Y, ball(0.6, 0.5)),
    (2, 1, v(0.0, 1.6, 0.0), DVec3::Y, ball(0.7, 1.0)),
    (3, 1, v(-0.28, 1.58, 0.0), DVec3::NEG_Y, ball(1.6, 1.0)),
    (4, 3, v(-0.28, 1.28, 0.0), DVec3::X, hinge(0.0, 2.5)),
    (5, 4, v(-0.28, 0.99, 0.0), DVec3::X, hinge(-1.0, 1.0)),
    (6, 0, v(-0.1, 0.9, 0.0), DVec3::NEG_Y, ball(1.4, 0.6)),
    (7, 6, v(-0.1, 0.5, 0.0), DVec3::X, hinge(-2.4, 0.0)),
    (8, 7, v(-0.1, 0.1, 0.0), DVec3::X, hinge(-0.6, 0.6)),
    (9, 1, v(0.28, 1.58, 0.0), DVec3::NEG_Y, ball(1.6, 1.0)),
    (10, 9, v(0.28, 1.28, 0.0), DVec3::X, hinge(0.0, 2.5)),
    (11, 10, v(0.28, 0.99, 0.0), DVec3::X, hinge(-1.0, 1.0)),
    (12, 0, v(0.1, 0.9, 0.0), DVec3::NEG_Y, ball(1.4, 0.6)),
    (13, 12, v(0.1, 0.5, 0.0), DVec3::X, hinge(-2.4, 0.0)),
    (14, 13, v(0.1, 0.1, 0.0), DVec3::X, hinge(-0.6, 0.6)),
];

/// The joints' damping (N·m·s/rad): a limp person's limbs settle in a swing or two.
const DAMPING: f64 = 2.0;
/// The segments' boots' and suit's friction.
const FRICTION: f64 = 0.8;

/// The head's segment.
pub const HEAD: usize = 2;

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
    // Each segment where it stands in the body's frame, its centre of mass on the body's.
    let feet = x - q * DVec3::Y * CENTRE;
    let masses: Vec<f64> = SEGMENTS.iter().map(|s| s.2 * mass).collect();
    let mut at: Vec<DVec3> = SEGMENTS.iter().map(|s| feet + q * s.0).collect();
    let centre = at.iter().zip(&masses).map(|(p, m)| *p * *m).sum::<DVec3>() / mass;
    for p in &mut at {
        *p += x - centre;
    }
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
        let at = feet + q * at + (x - centre);
        let axis = q * axis;
        let reference = axis.any_orthonormal_vector();
        let joint = Joint::new(
            world,
            ((parts[child], 0), Some((parts[parent], 0))),
            (at, axis, reference),
            kind,
        );
        let j = world.join(joint);
        world.joint_mut(j).damping = DAMPING;
    }
    parts
}

/// How far a character may lean before it has toppled (cosine of 45°).
const TOPPLED: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// The velocity change in a step what it meets may give it before it is knocked down (m/s):
/// people keep their feet by stepping after pushes that move their centre at 1–1.5 m/s, not
/// after one of 2 (walking into a wall is not one; running into it, or landing on the hips
/// from 3 m, is).
const STRUCK: f64 = 2.0;

/// Whether character `index` was knocked down in the last step (moving at `before` when it
/// began): toppled (leaning past 45° from `up`), or struck (jolted by more than [`STRUCK`]).
/// The jolt, not its contacts' forces: an impact's impulse is the impact law's, outside them.
pub fn knocked(world: &World, index: usize, up: DVec3, before: DVec3) -> bool {
    let s = &world.bodies[index].structure;
    if (s.orientation[0] * DVec3::Y).dot(up) < TOPPLED {
        return true;
    }
    super::injury::jolt(world, index, before).length() > STRUCK
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
