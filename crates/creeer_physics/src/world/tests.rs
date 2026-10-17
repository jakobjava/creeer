//! Contacts against the laws they must obey: resting, Newton's impact law, momentum
//! conservation, Coulomb friction, stacking, tumbling impacts, a crash.

use super::shape::{Shape, StaticShape};
use super::*;
use crate::structure::Section;
use crate::structure::section::STEEL_S235;
use crate::structure::shapes::{Base, space_frame};
use creeer_math::det;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;

fn ground() -> StaticShape {
    StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    }
}

/// A ball of radius 0.2 m and 10 kg dropped from a centre height of 1.2 m onto a ground
/// that gives back everything (restitution 1), so the ball's own `restitution` decides.
fn bounce(restitution: f64) -> (World, Vec<f64>) {
    let mut world = World::new(DT);
    world.statics.push(ground());
    world.static_restitution = 1.0;
    let mut ball = Body::rigid(Shape::Sphere { radius: 0.2 }, 10.0, DVec3::Y * 1.2, DT);
    ball.restitution = restitution;
    world.add(ball);
    let heights = (0..150)
        .map(|_| {
            world.step();
            world.bodies[0].structure.position[0].y
        })
        .collect();
    (world, heights)
}

/// Resting: the ball sinks in by the contact law's depth for m·g and stays still. Steel on
/// concrete is stiffer than a 60 Hz step resolves (`contact::stiffest`): a few µm.
#[test]
fn a_ball_rests_on_the_ground() {
    let (world, heights) = bounce(0.0);
    let sink = 0.2 - heights[149];
    let modulus = world.bodies[0]
        .surface
        .contact_modulus(&world.static_surface);
    let mut law = contact::law::Hertz::new(modulus, 0.2);
    law.set_cap(contact::cap(10.0, DT, 10.0 * 9.81));
    let want = law.depth(10.0 * G);
    assert!(
        (sink / want - 1.0).abs() < 0.01,
        "sank {sink:e} m, expected {want:e}"
    );
    assert!(world.bodies[0].structure.velocity[0].length() < 1e-6);
}

/// A soft ball (hard rubber) is well within what the step resolves: it rests at Hertz's own
/// depth, δ = (m·g/k)^⅔ with k = 4/3·E*·√R, 0.3 mm for 2 kg of radius 10 cm.
#[test]
fn a_rubber_ball_rests_at_hertz_depth() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    let mut ball = Body::rigid(Shape::Sphere { radius: 0.1 }, 2.0, DVec3::Y * 0.1, DT);
    ball.surface = surface::Surface::RUBBER;
    ball.restitution = 0.0;
    world.add(ball);
    for _ in 0..240 {
        world.step();
    }
    let sink = 0.1 - world.bodies[0].structure.position[0].y;
    let modulus = surface::Surface::RUBBER.contact_modulus(&world.static_surface);
    let law = contact::law::Hertz::new(modulus, 0.1);
    let want = law.depth(2.0 * G);
    assert!(
        law.stiffness(want) < 0.1 * contact::stiffest(2.0, DT),
        "resolved"
    );
    assert!(
        (sink / want - 1.0).abs() < 0.01,
        "sank {sink:e} m, expected {want:e}"
    );
    assert!(world.bodies[0].structure.velocity[0].length() < 1e-6);
}

/// Newton's impact law: the ball leaves with e times its impact speed, so it rises to
/// e²·(drop height).
#[test]
fn bounces_follow_the_restitution() {
    for e in [0.0, 0.3, 0.6, 0.9] {
        let (_, heights) = bounce(e);
        let touch = heights.iter().position(|&y| y < 0.2 + 1e-3).expect("lands");
        let peak = heights[touch..]
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        // Drop of 1 m from the release height to contact.
        let measured = (peak - 0.2).max(0.0).sqrt();
        assert!(
            (measured - e).abs() < 0.02,
            "e = {e}: measured {measured:.3}"
        );
    }
}

/// Two balls colliding head-on in free space: momentum is conserved exactly, and the
/// relative speed afterwards is e times the one before (6 m/s).
#[test]
fn collisions_conserve_momentum() {
    for e in [0.5, 1.0] {
        let mut world = World::new(DT);
        for (x, v, m) in [(-1.0, 4.0, 3.0), (1.0, -2.0, 5.0)] {
            let mut ball = Body::rigid(Shape::Sphere { radius: 0.25 }, m, DVec3::X * x, DT);
            ball.structure.gravity = DVec3::ZERO;
            ball.structure.velocity[0] = DVec3::X * v;
            ball.restitution = e;
            world.add(ball);
        }
        let before = world.momentum();
        for _ in 0..60 {
            world.step();
        }
        let after = world.momentum();
        assert!(
            (after - before).length() < 1e-9 * before.length(),
            "{before} -> {after}"
        );
        let v = |i: usize| world.bodies[i].structure.velocity[0].x;
        let measured = (v(1) - v(0)) / 6.0;
        assert!(
            (measured - e).abs() < 0.02,
            "e = {e}: measured {measured:.3}"
        );
    }
}

/// A 1 m box of 100 kg on a ramp tilted by `degrees`, friction μ on both surfaces; returns
/// how far it slid down the ramp between t = 1 s and t = 2 s.
fn slide(degrees: f64, mu: f64) -> f64 {
    let angle = degrees.to_radians();
    let (sin, cos) = det::sin_cos(angle);
    let normal = DVec3::new(-sin, cos, 0.0);
    let down = DVec3::new(-cos, -sin, 0.0);
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal,
        offset: 0.0,
    });
    world.static_friction = mu;
    let mut block = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        100.0,
        normal * 0.5,
        DT,
    );
    block.structure.orientation[0] = rotation(DVec3::Z, angle);
    block.friction = mu;
    world.add(block);
    let along = |w: &World| w.bodies[0].structure.position[0].dot(down);
    for _ in 0..60 {
        world.step();
    }
    let start = along(&world);
    for _ in 0..60 {
        world.step();
    }
    along(&world) - start
}

/// Coulomb friction: below the friction angle (tan 20° = 0.36 < μ = 0.6) the box stays put;
/// above it (40°, μ = 0.3) it slides with a = g·(sin θ − μ·cos θ) = 4.05 m/s².
#[test]
fn friction_on_a_ramp_follows_coulomb() {
    let stuck = slide(20.0, 0.6);
    assert!(
        stuck.abs() < 1e-4,
        "slid {stuck} m below the friction angle"
    );
    let (sin, cos) = det::sin_cos(40f64.to_radians());
    let a = G * (sin - 0.3 * cos);
    // Between t = 1 s and 2 s from rest: ½·a·(2² − 1²).
    let want = 0.5 * a * 3.0;
    let slid = slide(40.0, 0.3);
    assert!(
        (slid / want - 1.0).abs() < 0.02,
        "slid {slid} m, expected {want} m"
    );
}

/// Six 1 m boxes stacked on the ground stay put: after 5 s no box has drifted more than
/// 0.1 mm sideways and all are at rest.
#[test]
fn a_stack_of_boxes_is_stable() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    for k in 0..6 {
        let y = 0.5 + f64::from(k);
        world.add(Body::rigid(
            Shape::Box {
                half: DVec3::splat(0.5),
            },
            100.0,
            DVec3::Y * y,
            DT,
        ));
    }
    for _ in 0..300 {
        world.step();
    }
    for (k, body) in world.bodies.iter().enumerate() {
        let p = body.structure.position[0];
        assert!(
            DVec3::new(p.x, 0.0, p.z).length() < 1e-4,
            "box {k} drifted to {p}"
        );
        assert!(
            body.structure.velocity[0].length() < 1e-3,
            "box {k} still moving"
        );
    }
}

/// A box dropped corner-first lands, tumbles and comes to rest on a face, never having more
/// energy than it started with. (Single steps may rise by Newmark's bounded energy
/// oscillation, at most 0.1 % of the kinetic and elastic energy: the guard's tolerance.)
#[test]
fn a_tumbling_box_settles_without_gaining_energy() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    let half = DVec3::new(0.4, 0.2, 0.3);
    let mut block = Body::rigid(Shape::Box { half }, 20.0, DVec3::Y * 1.5, DT);
    block.structure.orientation[0] = rotation(DVec3::new(1.0, 0.3, 0.5), 0.7);
    block.structure.angular_velocity[0] = DVec3::new(1.0, 2.0, 0.0);
    world.add(block);
    let start = world.energy();
    for tick in 0..300 {
        world.step();
        let now = world.energy();
        assert!(now <= start * (1.0 + 1e-3), "tick {tick}: {now} > {start}");
    }
    let s = &world.bodies[0].structure;
    let height = s.position[0].y;
    assert!(
        [half.x, half.y, half.z]
            .iter()
            .any(|h| (height - h).abs() < 1e-3),
        "height {height}"
    );
    assert!(s.velocity[0].length() < 1e-3);
}

/// Crash test: a welded steel frame (a 4 × 2 × 2 m chassis) drives into a fixed wall at
/// 15 m/s. Nothing passes into the wall, energy only goes down, the frame stops, and its
/// members deform plastically.
///
/// "Into the wall" is measured against the penalty contact's own compliance (stiffness
/// k): a corner joint (mass m, three members of 1 m half length) arriving at speed v sinks
/// in by up to v·√(m/k) before it stops, plus N/k under its three members' squash load.
/// Anything beyond that would be the frame passing through.
#[test]
fn a_frame_crashing_into_a_wall_stops_without_passing_through() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    world.statics.push(StaticShape::Box {
        center: DVec3::new(6.0, 1.0, 1.0),
        rotation: DQuat::IDENTITY,
        half: DVec3::new(0.5, 2.0, 3.0),
    });
    let frame = space_frame(
        [2, 1, 1],
        2.0,
        Section::round_tube(0.08, 0.004),
        STEEL_S235,
        Base::Free,
    );
    let mut body = Body::structure(frame, DT);
    for p in &mut body.structure.position {
        p.y += 0.05;
    }
    for v in &mut body.structure.velocity {
        *v = DVec3::X * 15.0;
    }
    world.add(body);
    let start = world.energy();
    let mut energy = start;
    for tick in 0..120 {
        world.step();
        let now = world.energy();
        assert!(
            now <= energy + 1e-3 * start,
            "tick {tick}: energy rose {energy} -> {now}"
        );
        energy = now;
        // The wall's face is at x = 5.5; joints are balls of radius 4 cm.
        let section = Section::round_tube(0.08, 0.004);
        let joint = 3.0 * section.area * STEEL_S235.density;
        // A joint stopped from 15 m/s by the contact alone (½·m·v² = 2/5·k·δ^(5/2)), plus
        // what three yielding members press in.
        let modulus = world.bodies[0]
            .surface
            .contact_modulus(&world.static_surface);
        let law = contact::law::Hertz::new(modulus, 0.04);
        let impact = det::pow(5.0 * 0.5 * joint * 15.0 * 15.0 / (2.0 * law.k), 0.4);
        let sink = impact + law.depth(3.0 * STEEL_S235.yield_strength * section.area);
        let front = world.bodies[0]
            .structure
            .position
            .iter()
            .map(|p| p.x)
            .fold(f64::MIN, f64::max);
        assert!(
            front < 5.5 - 0.04 + sink,
            "tick {tick}: frame passed into the wall ({front})"
        );
    }
    let speed = world.bodies[0]
        .structure
        .velocity
        .iter()
        .map(|v| v.x)
        .fold(f64::MIN, f64::max);
    assert!(speed < 1.0, "still moving into the wall at {speed} m/s");
    let crushed = world.bodies[0]
        .structure
        .elements
        .iter()
        .map(|e| e.plastic_strain)
        .fold(0.0, f64::max);
    assert!(crushed > 1e-3, "no member deformed plastically ({crushed})");
}

/// A settled stack falls asleep (and costs nothing), stays exactly where it is, and wakes
/// all the way down when a ball hits its top.
#[test]
fn a_resting_stack_sleeps_and_wakes_when_hit() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    for k in 0..4 {
        let y = 0.5 + f64::from(k);
        world.add(Body::rigid(
            Shape::Box {
                half: DVec3::splat(0.5),
            },
            100.0,
            DVec3::Y * y,
            DT,
        ));
    }
    for _ in 0..180 {
        world.step();
    }
    assert_eq!(world.sleeping(), 4, "the settled stack sleeps");
    let resting: Vec<DVec3> = world
        .bodies
        .iter()
        .map(|b| b.structure.position[0])
        .collect();
    for _ in 0..60 {
        world.step();
    }
    let still: Vec<DVec3> = world
        .bodies
        .iter()
        .map(|b| b.structure.position[0])
        .collect();
    assert_eq!(resting, still, "sleeping bodies do not move");
    let mut ball = Body::rigid(
        Shape::Sphere { radius: 0.2 },
        50.0,
        DVec3::new(-3.0, 3.6, 0.0),
        DT,
    );
    ball.structure.velocity[0] = DVec3::new(8.0, 0.0, 0.0);
    world.add(ball);
    for _ in 0..30 {
        world.step();
    }
    assert_eq!(world.sleeping(), 0, "the hit woke the whole stack");
    assert!(
        world.bodies[3].structure.velocity[0].x > 0.1,
        "the top box was pushed"
    );
}
