//! Friction on faces pressed unevenly and in landings (ADR-0005, F5): the normal force
//! friction uses is the one that holds the body up, narrow bodies slide by Coulomb's law,
//! and a tumbling body's landing has friction.

use super::shape::{Shape, StaticShape};
use super::*;
use creeer_math::det;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;

/// A body of 100 kg, `half` its half extents, on a ramp of `degrees` with friction `mu`,
/// set down on it at rest.
fn on_ramp(half: DVec3, degrees: f64, mu: f64) -> (World, DVec3, DVec3) {
    let (sin, cos) = det::sin_cos(degrees.to_radians());
    let (normal, down) = (DVec3::new(-sin, cos, 0.0), DVec3::new(-cos, -sin, 0.0));
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal,
        offset: 0.0,
    });
    world.static_friction = mu;
    let mut body = Body::rigid(Shape::Box { half }, 100.0, normal * half.y, DT);
    body.structure.orientation[0] = rotation(DVec3::Z, degrees.to_radians());
    body.friction = mu;
    world.add(body);
    (world, normal, down)
}

/// A 0.4 m wide, 1 m tall box stuck on a 20° ramp presses its downhill edge harder and
/// lifts its uphill one; its corners' forces add up to the weight's part across the ramp,
/// m·g·cos θ, to 10⁻³ (4·10⁻⁵ measured; a step stops with its forces right to 1 %). (The
/// forces of corners lifted from the ramp while their face pressed
/// between them never reached the bodies: it rested on 965 N for its 921 N, and friction
/// held it with what was not there, F5.)
#[test]
fn an_unevenly_pressed_face_rests_on_its_weight() {
    let (mut world, ..) = on_ramp(DVec3::new(0.2, 0.5, 0.3), 20.0, 0.6);
    for _ in 0..60 {
        world.step();
    }
    let carried: f64 = world.contacts.iter().map(Contact::normal_force).sum();
    let want = 100.0 * G * det::cos(20f64.to_radians());
    assert!(
        (carried / want - 1.0).abs() < 1e-3,
        "{carried} N for {want} N"
    );
}

/// Narrow boxes (0.4, 0.6 and 1 m wide, 1 m tall) slide down a 40° ramp (μ = 0.3) with
/// a = g·(sin θ − μ·cos θ), within 1 %: the narrow ones on part of their face, friction
/// below their centre of mass lifting their uphill edge. (They slid 11 % and 3.5 % short
/// while that lifted part's forces were missing, F5.)
#[test]
fn narrow_boxes_slide_by_coulomb() {
    let (sin, cos) = det::sin_cos(40f64.to_radians());
    let want = 0.5 * G * (sin - 0.3 * cos) * 3.0;
    for width in [0.2, 0.3, 0.5] {
        let (mut world, _, down) = on_ramp(DVec3::new(width, 0.5, 0.3), 40.0, 0.3);
        for _ in 0..60 {
            world.step();
        }
        let start = world.bodies[0].structure.position[0].dot(down);
        for _ in 0..60 {
            world.step();
        }
        let slid = world.bodies[0].structure.position[0].dot(down) - start;
        assert!(
            (slid / want - 1.0).abs() < 0.01,
            "{width}: slid {slid} m, expected {want} m"
        );
    }
}

/// A cube tumbling down a 50° slope (μ = 1.5) lands on each face with friction: it keeps
/// turning the way it tumbled, slower, rather than sliding on with its spin turned back. (Its
/// landing was frictionless impulses at the leading edge, which reversed the spin and left
/// the cube sliding at 3.5 m/s, F5.)
#[test]
fn a_tumbling_cube_keeps_its_spin_when_it_lands() {
    let (mut world, ..) = on_ramp(DVec3::splat(0.25), 50.0, 1.5);
    let mut turned = 0.0;
    let mut previous = 0.0;
    for _ in 0..120 {
        world.step();
        let spin = world.bodies[0].structure.angular_velocity[0].z;
        turned += spin * DT;
        // The first landing: the spin drops after a quarter turn. Through it and a quarter of a
        // second after, the cube turns on the way it tumbled.
        if turned > 1.2 && spin < 0.9 * previous {
            let mut slowest = spin;
            for _ in 0..15 {
                world.step();
                slowest = slowest.min(world.bodies[0].structure.angular_velocity[0].z);
            }
            assert!(
                slowest > 0.0,
                "the landing turned the spin back: {previous} -> {slowest} rad/s"
            );
            return;
        }
        previous = spin;
    }
    panic!("the cube never landed (turned {turned} rad)");
}

/// Taking a body out of the world leaves the others' friction as it was: two crates held on
/// a 20° ramp (μ = 0.6) by their anchors, the first removed, the second stays put. (Before
/// F5 the bodies were removed from the list as it was, and the second took the first's
/// anchors: they pulled it 3.8 mm in 0.5 s; now it moves 2 nm.)
#[test]
fn removing_a_body_leaves_the_others_anchored() {
    let half = DVec3::splat(0.5);
    let (mut world, normal, down) = on_ramp(half, 20.0, 0.6);
    let across = normal.cross(down);
    let mut second = world.bodies[0].clone();
    second.structure.position[0] += across * 3.0;
    second.solver.reset_history(&second.structure);
    world.add(second);
    for _ in 0..30 {
        world.step();
    }
    let at = world.bodies[1].structure.position[0];
    world.remove_body(0);
    world.bodies[0].wake();
    for _ in 0..30 {
        world.step();
    }
    let moved = (world.bodies[0].structure.position[0] - at).length();
    assert!(moved < 1e-6, "moved {moved} m");
    // The same for removing all but one.
    let (mut world, ..) = on_ramp(half, 20.0, 0.6);
    let mut second = world.bodies[0].clone();
    second.structure.position[0] += across * 3.0;
    second.solver.reset_history(&second.structure);
    world.add(second);
    for _ in 0..30 {
        world.step();
    }
    let at = world.bodies[1].structure.position[0];
    let map = world.retain_bodies(|i, _| i == 1);
    assert_eq!(map, vec![None, Some(0)]);
    world.bodies[0].wake();
    for _ in 0..30 {
        world.step();
    }
    let moved = (world.bodies[0].structure.position[0] - at).length();
    assert!(moved < 1e-6, "moved {moved} m");
}

/// A ball of 50 kg set spinning at 0.2 rad/s about the vertical on the ground slows at a
/// steady rate, as sliding friction over its contact circle does (torque μ·N·3πa/16, a the
/// circle's radius, `contact/twist.rs`): the torque it takes is that of a lever of a
/// fraction of a millimetre to a few millimetres. Then it stops, and sleeps. (A point
/// contact's friction has no lever about its normal: it spun on for good, F6.)
#[test]
fn a_ball_spinning_on_the_ground_stops() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world.static_friction = 0.6;
    let (radius, mass) = (0.3, 50.0);
    let mut ball = Body::rigid(Shape::Sphere { radius }, mass, DVec3::Y * radius, DT);
    ball.friction = 0.6;
    world.add(ball);
    for _ in 0..60 {
        world.step();
    }
    world.bodies[0].structure.angular_velocity[0] = DVec3::Y * 0.2;
    world.bodies[0].wake();
    let spin = |w: &World| w.bodies[0].structure.angular_velocity[0].y;
    let mut rates = Vec::new();
    for _ in 0..3 {
        let before = spin(&world);
        for _ in 0..15 {
            world.step();
        }
        rates.push((before - spin(&world)) / 0.25);
    }
    // Steady: Coulomb's law, not a dashpot's exponential.
    let (first, last) = (rates[0], rates[2]);
    assert!(
        first > 0.0 && (last / first - 1.0).abs() < 0.05,
        "{rates:?}"
    );
    let inertia = 0.4 * mass * radius * radius;
    let lever = first * inertia / (0.6 * mass * G);
    assert!(lever > 1e-4 && lever < 5e-3, "lever {lever} m");
    for _ in 0..1200 {
        world.step();
    }
    assert!(world.bodies[0].sleep.asleep, "spins at {}", spin(&world));
}

#[path = "stick_tests.rs"]
mod stick;
