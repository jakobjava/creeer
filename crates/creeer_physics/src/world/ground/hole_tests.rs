//! Ground dug into a narrow hole with a slot beside it (G2.10): where two digs meet, their
//! rounds' gradient is a weighted mean of two gradients pointing apart, far shorter than a
//! distance's (`ground/projection.rs`). Points there are projected onto the ground, and a pole
//! lying in the slot with its foot in the hole and turning in it gains nothing from its
//! contacts. (The projection's step d/|∇d| reached 0.8 m along the slot from a point in the
//! air, its foot's tangent plane put the point 9 cm into the ground, and the pole's face patches
//! found 602 kJ at a step's start: thrown at 11 m/s, `creeer_server`'s `raising_tests.rs`.)

use crate::world::ground::{self, Ground};
use crate::world::hull::Hull;
use crate::world::planet_tests::FieldGround;
use crate::world::shape::Shape;
use crate::world::*;
use creeer_math::det;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;

/// Distance from `p` to the capsule `a`–`b` of `radius` (negative inside).
fn capsule(p: DVec3, a: DVec3, b: DVec3, radius: f64) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p - (a + ab * t)).length() - radius
}

/// A dig into density `d` by a brush at distance `brush`, rounded over `k` as the terrain's
/// edits are (`creeer_terrain::edits`: max(d, −brush) by Quilez's polynomial smooth minimum).
fn dig(d: f64, brush: f64, k: f64) -> f64 {
    let h = (k - (-d - brush).abs()).max(0.0) / k;
    -((-d).min(brush) - 0.25 * k * h * h)
}

/// The pole's foot, 20 cm down the hole, and the way west along the slot (rising 8°).
fn foot_and_west() -> (DVec3, DVec3) {
    let (s, c) = det::sin_cos(8f64.to_radians());
    (DVec3::new(0.0, -0.2, 0.0), DVec3::new(-c, s, 0.0))
}

/// Level ground (y up) with a hole dug at the origin (a capsule 15 cm in radius, 0.7 m deep)
/// and a slot as wide beside it along the pole lying in it, its floor 1 cm under the pole and
/// out to where the pole clears the ground (as `creeer_server`'s `raising_tests.rs` digs
/// them), each rounded over the brush's radius as the terrain rounds a brush that narrow.
fn dug(p: DVec3) -> f64 {
    let (foot, west) = foot_and_west();
    let floor = DVec3::Y * 0.08;
    let hole = capsule(p, DVec3::ZERO, DVec3::new(0.0, -0.7, 0.0), 0.15);
    let slot = capsule(p, foot + floor, foot + west * 2.05 + floor, 0.15);
    dig(dig(p.y, hole, 0.15), slot, 0.15)
}

/// Around where the slot meets the hole, every point is projected onto the ground: its foot
/// on the surface (the density there within 0.1 mm of none), or, where no path reaches it (on
/// the slot's mid-plane above its axis, open to the sky), taken as clear by as much as the
/// field guarantees; the point on its side of the tangent plane there, and the foot no nearer
/// than the field allows (it rises at most a metre a metre: its parts' slopes are 1 and their
/// rounds' weighted means are less). Many of the points see a gradient shorter than 0.9 there.
/// (Projected as before G2.10, 3,156 of 12,733 were given feet off the ground, up to 1.66 m,
/// and 142 in the air were put up to 22.5 cm into it.)
#[test]
fn a_point_where_two_digs_meet_is_projected_onto_the_ground() {
    let ground = FieldGround(dug);
    let (mut shallow, mut points, mut clear, mut off) = (0, 0, 0, 0.0f64);
    for i in 0..=36 {
        for j in 0..=28 {
            for k in 0..=12 {
                let p = DVec3::new(
                    -0.6 + 0.025 * f64::from(i),
                    -0.5 + 0.025 * f64::from(j),
                    -0.15 + 0.025 * f64::from(k),
                );
                let (d, g) = ground.density_gradient(p);
                if d.abs() < 1e-3 || d.abs() > 0.2 {
                    continue;
                }
                points += 1;
                shallow += usize::from(g.length() < 0.9);
                // None only on the hole's axis itself, where the field has no slope (ADR-0005
                // G2.9).
                let Some((foot, n)) = ground::project(&ground, p) else {
                    assert!(g.length() < 1e-6, "{p:?}: no ground, |∇| {:.3}", g.length());
                    continue;
                };
                let gap = n.dot(p - foot);
                let on = dug(foot).abs() < 1e-4;
                let guaranteed = (gap - d / g.length().max(1.0)).abs() < 1e-12;
                off = off.max(if on { 0.0 } else { dug(foot).abs() });
                clear += usize::from(!on);
                assert!(
                    on || guaranteed,
                    "{p:?} (density {d:.4}, |∇| {:.3}): its foot {foot:?} is {:.4} off the ground",
                    g.length(),
                    dug(foot)
                );
                assert!(
                    gap * d > 0.0 && (p - foot).length() > d.abs() - 1e-4,
                    "{p:?} (density {d:.4}, |∇| {:.3}): {gap:.4} m from its foot {foot:?}",
                    g.length()
                );
            }
        }
    }
    eprintln!(
        "{points} points, {shallow} shallow; {clear} taken as clear by the field's guarantee \
         (their steps' end at most {off:.3} off the ground)"
    );
    assert!(
        shallow > 100,
        "{shallow} of {points} points saw a shallow gradient"
    );
}

/// A 3 m green spruce pole (a 16-sided prism, 12 cm thick, 27 kg) lying in the slot with its
/// foot 20 cm down the hole, at rest, swung up about its foot as a person raises it, and
/// rolled about its own axis: over two seconds its energy (motion, gravity's and what its
/// contacts store) never rises more than 10 mJ above where it started, and it is never faster
/// than its start's energy lets it be. (Projected as before G2.10, swung up it gained 53 MJ;
/// with the steps bounded but the feet not the nearest, swung and rolled it gained 2.6 kJ,
/// stopped dead by the energy guard again and again.)
#[test]
fn a_pole_turning_in_a_slot_beside_a_hole_gains_nothing() {
    let (foot, west) = foot_and_west();
    let ring: Vec<DVec3> = (0..16)
        .map(|k| {
            let (s, c) = det::sin_cos(std::f64::consts::TAU * f64::from(k) / 16.0);
            DVec3::new(0.0, 0.06 * c, 0.06 * s)
        })
        .collect();
    let corners: Vec<DVec3> = ring
        .iter()
        .flat_map(|&r| [r + DVec3::X * 1.5, r - DVec3::X * 1.5])
        .collect();
    let (hull, _) = Hull::new(&corners).expect("a prism");
    let hull = Arc::new(hull);
    let mass = 800.0 * hull.volume();
    let north = DVec3::Z;
    for (case, (swing, roll)) in [(0.0, 0.0), (1.5, 0.0), (0.0, 4.0), (1.5, 4.0)]
        .into_iter()
        .enumerate()
    {
        let mut world = World::new(DT);
        world.ground = Some(Arc::new(FieldGround(dug)));
        world.static_friction = 0.6;
        let centre = foot + west * 1.5;
        let mut pole = Body::rigid(Shape::Hull(hull.clone()), mass, centre, DT);
        // Its foot (+x) in the hole, its top west; swung about the foot (its top rising), and
        // rolled about its axis.
        pole.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::X, -west);
        let w = north * -swing - west * roll;
        pole.structure.angular_velocity[0] = w;
        pole.structure.velocity[0] = w.cross(centre - foot);
        pole.surface = surface::Surface::new(9.0e9, 0.3);
        pole.friction = 0.6;
        world.add(pole);
        let start = world.energy();
        let lowest = -0.7 - 0.15;
        let fall = mass * G * (centre.y - lowest);
        let fastest_allowed = (2.0 * (world.bodies[0].structure.energy() + fall) / mass).sqrt();
        let (mut rise, mut fastest) = (0.0f64, 0.0f64);
        for _ in 0..120 {
            world.step();
            rise = rise.max(world.energy() - start);
            fastest = fastest.max(world.bodies[0].structure.velocity[0].length());
        }
        eprintln!(
            "case {case} (swung {swing} rad/s, rolled {roll}): energy up {rise:.4} J at most, \
             fastest {fastest:.2} m/s (at most {fastest_allowed:.2})"
        );
        assert!(rise < 0.01, "case {case}: its energy rose {rise:.3} J");
        assert!(fastest < fastest_allowed, "case {case}: {fastest:.2} m/s");
    }
}
