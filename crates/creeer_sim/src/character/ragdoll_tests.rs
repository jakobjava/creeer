//! The ragdoll: its handover keeps the body's momentum and angular momentum, its segments
//! weigh what the body did, and a fallen ragdoll comes to rest on the ground, its joints held.

use super::*;
use creeer_math::glam::DQuat;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;

/// Momentum and angular momentum (about the origin) of `bodies`.
fn momenta(world: &World, bodies: &[usize]) -> (DVec3, DVec3) {
    let mut out = (DVec3::ZERO, DVec3::ZERO);
    for &b in bodies {
        let s = &world.bodies[b].structure;
        let (m, r) = (s.masses()[0], DMat3::from_quat(s.orientation[0]));
        out.0 += s.velocity[0] * m;
        out.1 += s.position[0].cross(s.velocity[0] * m)
            + r * s.inertia_of(0) * r.transpose() * s.angular_velocity[0];
    }
    out
}

#[test]
fn the_segments_weigh_what_the_body_did() {
    let total: f64 = SEGMENTS.iter().map(|s| s.2).sum();
    assert!((total - 1.0).abs() < 1e-12, "the shares sum to {total}");
}

#[test]
fn segments_not_joined_stand_apart() {
    // Joined segments do not collide; any other two overlapping as they stand would be thrown
    // apart by their contacts the moment the ragdoll forms. The closest pair, a forearm and a
    // thigh as the arms hang by the hips, is a centimetre apart.
    let joined = |a: usize, b: usize| {
        LINKS
            .iter()
            .any(|l| (l.0, l.1) == (a, b) || (l.0, l.1) == (b, a))
    };
    let mut closest = f64::INFINITY;
    for (a, (ca, ha, _)) in SEGMENTS.iter().enumerate() {
        for (b, (cb, hb, _)) in SEGMENTS.iter().enumerate().skip(a + 1) {
            if joined(a, b) {
                continue;
            }
            let gap = ((*ca - *cb).abs() - (*ha + *hb)).max_element();
            assert!(gap > 0.0, "segments {a} and {b} overlap by {}", -gap);
            closest = closest.min(gap);
        }
    }
    assert!(closest < 0.02, "the closest pair is {closest} m apart");
}

#[test]
fn falling_keeps_momentum_and_angular_momentum() {
    let mut world = World::new(DT);
    let mut b = character::body(DVec3::new(1.0, 2.0, -0.5), DVec3::Y, DT);
    let s = &mut b.structure;
    s.velocity[0] = DVec3::new(3.0, -1.0, 0.5);
    s.angular_velocity[0] = DVec3::new(0.5, 0.2, -2.0);
    s.orientation[0] = DQuat::from_rotation_z(0.4);
    let c = world.add(b);
    let (p0, l0) = momenta(&world, &[c]);
    let parts = fall(&mut world, c);
    assert_eq!(parts.len(), 15);
    assert_eq!(parts[0], c);
    let (p, l) = momenta(&world, &parts);
    assert!(
        (p - p0).length() < 1e-9 * p0.length(),
        "momentum {p0} → {p}"
    );
    assert!(
        (l - l0).length() < 1e-9 * l0.length(),
        "angular momentum {l0} → {l}"
    );
    assert_eq!(world.joints().len(), 14);
    // Its limbs pass through each other, but for the arms on the trunk and head.
    assert!(world.passes(parts[6], parts[12]) && world.passes(parts[5], parts[6]));
    assert!(!world.passes(parts[4], parts[1]) && !world.passes(parts[11], parts[HEAD]));
}

#[test]
fn a_fallen_ragdoll_comes_to_rest_with_its_joints_held() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let mut b = character::body(DVec3::new(0.0, 0.02, 0.0), DVec3::Y, DT);
    // Toppling forwards.
    b.structure.angular_velocity[0] = DVec3::new(1.5, 0.0, 0.0);
    let c = world.add(b);
    let parts = fall(&mut world, c);
    for _ in 0..600 {
        world.step();
    }
    assert!(
        parts.iter().all(|&p| world.bodies[p].sleep.asleep),
        "still moving after 10 s"
    );
    // Lying on the ground: every segment low, the joints shut.
    for &p in &parts {
        let y = world.bodies[p].structure.position[0].y;
        assert!(y < 0.4, "segment {p} at {y} m");
    }
    let worst = (world.joints().iter())
        .map(|j| {
            let point = |(b, n): (usize, usize), local: DVec3| {
                let s = &world.bodies[b].structure;
                s.position[n] + s.orientation[n] * local
            };
            (point(j.a, j.at_a) - point(j.b.expect("between segments"), j.at_b)).length()
        })
        .fold(0.0, f64::max);
    assert!(worst < 1e-3, "a joint opened {worst} m");
    // Up again where the pelvis lies.
    let feet = DVec3::new(world.bodies[c].structure.position[0].x, 0.02, 0.0);
    let up = standing(&world, &parts, feet, DVec3::Y);
    let mass: f64 = parts
        .iter()
        .map(|&p| world.bodies[p].structure.masses()[0])
        .sum();
    assert!((up.structure.masses()[0] - mass).abs() < 1e-9);
}
