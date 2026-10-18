//! Joints against mechanics (ADR-0022): a pendulum's period, a double pendulum against an
//! RK4 reference in its own two angles, energy never rising, momentum kept in free flight,
//! a chain at rest staying at rest with its weight in its top joint; and the world's
//! bookkeeping: joined bodies not colliding, joints saved and renumbered.

use super::{Joint, Kind};
use crate::world::shape::Shape;
use crate::world::{Body, World};
use creeer_math::glam::DVec3;

mod motion;

pub(super) const DT: f64 = 1.0 / 60.0;
pub(super) const G: f64 = 9.806_65;

/// A ball of `mass` and radius 5 cm at `at`.
pub(super) fn ball(world: &mut World, at: DVec3, mass: f64) -> usize {
    let dt = world.dt();
    world.add(Body::rigid(Shape::Sphere { radius: 0.05 }, mass, at, dt))
}

/// A ball joint between `a` and `b` (or the world) at `at`, twist axis down.
pub(super) fn ball_joint(world: &mut World, a: usize, b: Option<usize>, at: DVec3) -> usize {
    let kind = Kind::Ball {
        cone: None,
        twist: None,
    };
    let joint = Joint::new(
        world,
        ((a, 0), b.map(|b| (b, 0))),
        (at, DVec3::NEG_Y, DVec3::X),
        kind,
    );
    world.join(joint)
}

/// The angle of a pendulum's bob from the vertical below its pivot (rad, in the x–y plane).
pub(super) fn angle(pivot: DVec3, bob: DVec3) -> f64 {
    let d = bob - pivot;
    creeer_math::det::atan2(d.x, -d.y)
}

#[test]
fn a_pendulum_swings_with_its_period() {
    let mut world = World::new(DT);
    let theta: f64 = 5f64.to_radians();
    let bob = ball(
        &mut world,
        DVec3::new(
            creeer_math::det::sin(theta),
            -creeer_math::det::cos(theta),
            0.0,
        ),
        1.0,
    );
    ball_joint(&mut world, bob, None, DVec3::ZERO);
    // A compound pendulum: the ball's own inertia about its centre (2/5·m·r²) with m·L²; the
    // amplitude lengthens it by θ₀²/16.
    let inertia = 0.4 * 0.05 * 0.05 + 1.0;
    let period = std::f64::consts::TAU * (inertia / G).sqrt() * (1.0 + theta * theta / 16.0);
    // Upward crossings of the vertical, found between steps.
    let mut crossings = Vec::new();
    let mut last = angle(DVec3::ZERO, world.bodies[bob].structure.position[0]);
    for step in 1..=900 {
        world.step();
        let now = angle(DVec3::ZERO, world.bodies[bob].structure.position[0]);
        if last < 0.0 && now >= 0.0 {
            let fraction = -last / (now - last);
            crossings.push((f64::from(step) - 1.0 + fraction) * DT);
        }
        last = now;
    }
    assert!(crossings.len() >= 6, "{} crossings", crossings.len());
    let measured = (crossings[crossings.len() - 1] - crossings[0]) / (crossings.len() - 1) as f64;
    assert!(
        (measured / period - 1.0).abs() < 1e-3,
        "period {measured} s against {period} s"
    );
    // The bob stays on its circle.
    let length = world.bodies[bob].structure.position[0].length();
    assert!((length - 1.0).abs() < 1e-6, "rod length {length}");
}

#[test]
fn a_chain_at_rest_stays_at_rest_with_its_weight_in_its_top_joint() {
    let mut world = World::new(DT);
    let links = 15;
    let bodies: Vec<usize> = (0..links)
        .map(|k| {
            ball(
                &mut world,
                DVec3::new(0.0, -0.1 * (k as f64 + 1.0), 0.0),
                1.0,
            )
        })
        .collect();
    let top = ball_joint(&mut world, bodies[0], None, DVec3::ZERO);
    for k in 1..links {
        let at = DVec3::new(0.0, -0.1 * k as f64 - 0.05, 0.0);
        ball_joint(&mut world, bodies[k], Some(bodies[k - 1]), at);
    }
    let start: Vec<DVec3> = (bodies.iter())
        .map(|&b| world.bodies[b].structure.position[0])
        .collect();
    for _ in 0..300 {
        world.step();
    }
    for (&b, s) in bodies.iter().zip(&start) {
        let st = &world.bodies[b].structure;
        let moved = (st.position[0] - *s).length();
        assert!(moved < 1e-5, "body {b} moved {moved} m");
        assert!(st.velocity[0].length() < 1e-5, "body {b} still moves");
    }
    // The world holds the chain up: on the top ball it pushes with the chain's weight.
    let weight = G * links as f64;
    let held = world.joints()[top].force();
    assert!(
        (held - DVec3::Y * weight).length() < 1e-3 * weight,
        "the top joint holds {held} N of {weight}"
    );
}

#[test]
fn energy_never_rises_in_a_falling_chain() {
    let mut world = World::new(DT);
    let bodies: Vec<usize> = (0..5)
        .map(|k| {
            ball(
                &mut world,
                DVec3::new(0.3 * (k as f64 + 1.0), 0.0, 0.0),
                1.0,
            )
        })
        .collect();
    ball_joint(&mut world, bodies[0], None, DVec3::ZERO);
    for k in 1..5 {
        let at = DVec3::new(0.3 * k as f64 + 0.15, 0.0, 0.0);
        ball_joint(&mut world, bodies[k], Some(bodies[k - 1]), at);
    }
    let start = world.energy();
    let (mut highest, mut kinetic) = (start, 0.0f64);
    for _ in 0..240 {
        world.step();
        highest = highest.max(world.energy());
        let k: f64 = (world.bodies.iter())
            .map(|b| b.structure.node_energy().0)
            .sum();
        kinetic = kinetic.max(k);
    }
    assert!(kinetic > 10.0, "the chain fell ({kinetic} J)");
    assert!(
        highest - start <= 1e-3 * kinetic,
        "energy rose by {} J",
        highest - start
    );
    // Every joint holds its two points together.
    let gap = widest_gap(&world);
    assert!(gap < 1e-5, "a joint opened {gap} m");
}

/// Two boxes joined by a ball joint, tumbling through two seconds without gravity at a
/// step of `dt`: (momentum before, after; angular momentum before, after; widest gap).
fn tumble(dt: f64) -> (DVec3, DVec3, DVec3, DVec3, f64) {
    let mut world = World::new(dt);
    let shape = |half| Shape::Box { half };
    let mut a = Body::rigid(
        shape(DVec3::new(0.3, 0.1, 0.1)),
        2.0,
        DVec3::new(-0.3, 0.0, 0.0),
        dt,
    );
    let mut b = Body::rigid(
        shape(DVec3::new(0.2, 0.15, 0.1)),
        3.0,
        DVec3::new(0.2, 0.0, 0.0),
        dt,
    );
    // B's velocity is A's at the joint (the origin): the joint holds from the start.
    let (va, wa, wb) = (
        DVec3::new(1.0, 0.5, 0.0),
        DVec3::new(0.0, 2.0, 1.0),
        DVec3::new(1.0, -1.0, 0.0),
    );
    let vb = va + wa.cross(DVec3::new(0.3, 0.0, 0.0)) - wb.cross(DVec3::new(-0.2, 0.0, 0.0));
    for (body, v, w) in [(&mut a, va, wa), (&mut b, vb, wb)] {
        body.structure.gravity = DVec3::ZERO;
        body.structure.velocity[0] = v;
        body.structure.angular_velocity[0] = w;
    }
    let (a, b) = (world.add(a), world.add(b));
    ball_joint(&mut world, b, Some(a), DVec3::ZERO);
    let angular = |world: &World| -> DVec3 {
        (world.bodies.iter())
            .map(|body| {
                let s = &body.structure;
                let r = creeer_math::glam::DMat3::from_quat(s.orientation[0]);
                s.position[0].cross(s.velocity[0] * s.mass[0])
                    + r * s.inertia[0] * r.transpose() * s.angular_velocity[0]
            })
            .sum()
    };
    let (p0, l0) = (world.momentum(), angular(&world));
    let mut gap = 0.0f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(2.0 / dt).round() as usize {
        world.step();
        gap = gap.max(widest_gap(&world));
    }
    (p0, world.momentum(), l0, angular(&world), gap)
}

#[test]
fn momentum_is_kept_in_free_flight() {
    // Momentum exactly (the joint's forces are equal and opposite). Angular momentum within
    // what its gap allows: the forces act where each side's point is, a gap (within
    // `HOLDS`) apart, a torque of gap × force (2 s of 10⁻⁶ m under ~10 N: 5·10⁻⁵ of it).
    for dt in [DT, DT / 4.0] {
        let (p0, p, l0, l, gap) = tumble(dt);
        assert!(
            (p - p0).length() < 1e-9 * p0.length(),
            "momentum {p0} → {p}"
        );
        assert!(gap < 2.0 * super::HOLDS, "the joint opened {gap} m");
        let off = (l - l0).length() / l0.length();
        assert!(off < 1e-4, "angular momentum {off} off at {dt} s");
    }
}

#[test]
fn joined_bodies_do_not_collide_unless_asked() {
    for collide in [false, true] {
        let mut world = World::new(DT);
        let dt = world.dt();
        let boxes: Vec<usize> = [DVec3::ZERO, DVec3::new(0.3, 0.0, 0.0)]
            .into_iter()
            .map(|at| {
                let mut b = Body::rigid(
                    Shape::Box {
                        half: DVec3::splat(0.2),
                    },
                    1.0,
                    at,
                    dt,
                );
                b.structure.gravity = DVec3::ZERO;
                world.add(b)
            })
            .collect();
        let j = ball_joint(
            &mut world,
            boxes[1],
            Some(boxes[0]),
            DVec3::new(0.15, 0.0, 0.0),
        );
        world.joint_mut(j).collide = collide;
        world.step();
        assert_eq!(!world.contacts.is_empty(), collide, "collide {collide}");
    }
}

#[test]
fn joints_are_saved_and_follow_their_bodies() {
    let mut world = World::new(DT);
    let a = ball(&mut world, DVec3::new(1.0, 0.0, 0.0), 1.0);
    let b = ball(&mut world, DVec3::new(2.0, 0.0, 0.0), 2.0);
    let hinge = Joint::new(
        &world,
        ((a, 0), None),
        (DVec3::ZERO, DVec3::Z, DVec3::X),
        Kind::Hinge {
            range: Some((-0.3, 0.6)),
        },
    );
    world.join(hinge);
    ball_joint(&mut world, b, Some(a), DVec3::new(1.5, 0.0, 0.0));
    world.joint_mut(1).damping = 0.5;
    for _ in 0..30 {
        world.step();
    }
    let bytes = world.to_bytes();
    let mut loaded = World::from_bytes(&bytes).unwrap();
    assert_eq!(loaded.to_bytes(), bytes);
    assert_eq!(loaded.joints(), world.joints());
    // It goes on bit for bit.
    for _ in 0..30 {
        world.step();
        loaded.step();
    }
    assert_eq!(loaded.to_bytes(), world.to_bytes());
    // A body removed takes its joints with it; the others follow their bodies.
    let mut world = loaded;
    world.remove_body(a);
    assert!(world.joints().is_empty());
    let mut world = World::from_bytes(&bytes).unwrap();
    let c = ball(&mut world, DVec3::new(0.0, 3.0, 0.0), 1.0);
    ball_joint(&mut world, c, None, DVec3::new(0.0, 4.0, 0.0));
    world.remove_body(b);
    assert_eq!(world.joints().len(), 2);
    assert_eq!(world.joints()[1].a, (c - 1, 0));
}

#[test]
fn a_hinge_rests_on_its_stop_with_the_torque_of_its_load() {
    let mut world = World::new(DT);
    let bob = ball(&mut world, DVec3::new(1.0, 0.0, 0.0), 1.0);
    // About z, its reference towards the ball: the ball falling turns it the positive way.
    let hinge = Joint::new(
        &world,
        ((bob, 0), None),
        (DVec3::ZERO, DVec3::Z, DVec3::X),
        Kind::Hinge {
            range: Some((-0.3, 0.3)),
        },
    );
    let j = world.join(hinge);
    for _ in 0..300 {
        world.step();
    }
    let at = world.bodies[bob].structure.position[0];
    let fallen = creeer_math::det::atan2(-at.y, at.x);
    assert!((fallen - 0.3).abs() < 1e-4, "rests at {fallen} rad");
    // The stop carries the load's torque, m·g·L·cos(0.3).
    let torque = world.joints()[j].stop_torque(1);
    let want = G * creeer_math::det::cos(0.3f64);
    assert!(
        (torque / want - 1.0).abs() < 1e-3,
        "{torque} N·m against {want}"
    );
}

/// The largest gap between any joint's two points (m).
pub(super) fn widest_gap(world: &World) -> f64 {
    world
        .joints()
        .iter()
        .map(|j| {
            let point = |side: Option<(usize, usize)>, local: DVec3| match side {
                Some((b, n)) => {
                    let s = &world.bodies[b].structure;
                    s.position[n] + s.orientation[n] * local
                }
                None => local,
            };
            (point(Some(j.a), j.at_a) - point(j.b, j.at_b)).length()
        })
        .fold(0.0, f64::max)
}
