//! Struts: a body on one rests at its spring's compression under its weight; a heavy body on
//! one standing on a light plate on the ground presses the plate down and does not throw it
//! (a load put on the plate before the step did); between two free bodies one keeps their
//! momentum, pushes only, and at most its most; its damper damps as a damped oscillator's.

use super::shape::{Shape, StaticShape};
use super::strut::Strut;
use super::{Body, World};
use creeer_math::glam::DVec3;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.81;

fn floor() -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world
}

fn ball(at: DVec3, mass: f64) -> Body {
    let mut b = Body::rigid(Shape::Sphere { radius: 0.2 }, mass, at, DT);
    b.structure.gravity = DVec3::new(0.0, -G, 0.0);
    b.solver.reset_history(&b.structure);
    b
}

fn leg(a: usize, b: Option<(usize, usize)>, at_b: DVec3) -> Strut {
    Strut {
        a: (a, 0),
        at_a: DVec3::ZERO,
        b,
        at_b,
        direction: DVec3::Y,
        rest: 1.0,
        stiffness: 30_000.0,
        damping: 2000.0,
        most: 3600.0,
    }
}

/// Steps `world` `n` times with `struts` set for each step (as their owner sets them).
fn run(world: &mut World, struts: &[Strut], n: usize) {
    for _ in 0..n {
        for s in struts {
            world.add_strut(*s);
        }
        world.step();
        world.clear_struts();
    }
}

#[test]
fn a_body_on_a_strut_rests_at_its_springs_compression() {
    let mut world = World::new(DT);
    let m = 105.0;
    let a = world.add(ball(DVec3::new(0.0, 1.0, 0.0), m));
    run(&mut world, &[leg(a, None, DVec3::ZERO)], 300);
    let s = &world.bodies[a].structure;
    let sag = 1.0 - s.position[0].y;
    let expected = m * G / 30_000.0;
    assert!(
        (sag - expected).abs() < 1e-4,
        "sags {sag} m, the spring {expected}"
    );
    assert!(
        s.velocity[0].length() < 1e-3,
        "still moving at {}",
        s.velocity[0]
    );
}

#[test]
fn a_heavy_body_standing_on_a_light_plate_presses_it_down_unthrown() {
    let mut world = floor();
    // A plate of 0.6 kg (a hand's) on the floor, a body of 105 kg standing on it by a strut.
    let mut plate = Body::rigid(
        Shape::Box {
            half: DVec3::new(0.05, 0.025, 0.045),
        },
        0.6,
        DVec3::new(0.0, 0.025, 0.0),
        DT,
    );
    plate.structure.gravity = DVec3::new(0.0, -G, 0.0);
    plate.friction = 0.8;
    let plate = world.add(plate);
    let body = world.add(ball(DVec3::new(0.0, 1.05, 0.0), 105.0));
    // Off the plate's centre, as a boot's heel would stand.
    let heel = DVec3::new(0.03, 0.025, 0.01);
    let strut = Strut {
        rest: 1.0 + 105.0 * G / 30_000.0,
        ..leg(body, Some((plate, 0)), heel)
    };
    // The first step the floor takes up a kilonewton off the plate's centre from nothing: it
    // rocks once (2.8 rad/s; as a load put on before the step, 64 rad/s, and thrown), then
    // lies still.
    let (mut first, mut fastest, mut weight) = (0.0, 0.0f64, 0.0);
    for step in 0..240 {
        run(&mut world, &[strut], 1);
        let s = &world.bodies[plate].structure;
        let speed = s.velocity[0].length() + s.angular_velocity[0].length() * 0.05;
        if step == 0 {
            first = s.angular_velocity[0].length();
        } else if step == 20 {
            weight = world.supports(plate).iter().map(|c| c.force).sum();
        } else if step > 2 {
            fastest = fastest.max(speed);
        }
    }
    let s = &world.bodies[plate].structure;
    assert!(first < 5.0, "the plate turned at {first} rad/s");
    assert!(fastest < 0.01, "the plate moved at up to {fastest} m/s");
    assert!(s.position[0].distance(DVec3::new(0.0, 0.025, 0.0)) < 2e-3);
    // The floor carries both (while they are awake).
    assert!(
        (weight - 105.6 * G).abs() < 0.02 * 105.6 * G,
        "the floor carries {weight} N"
    );
}

#[test]
fn between_free_bodies_it_keeps_momentum_pushes_only_and_at_most_its_most() {
    let mut world = World::new(DT);
    let mut free = |at: DVec3, mass: f64| {
        let mut b = ball(at, mass);
        b.structure.gravity = DVec3::ZERO;
        b.solver.reset_history(&b.structure);
        world.add(b)
    };
    let (a, b) = (
        free(DVec3::new(0.0, 0.5, 0.0), 2.0),
        free(DVec3::ZERO, 50.0),
    );
    // Pressed 0.5 m: 15 kN asked, 3.6 kN at most.
    let strut = leg(a, Some((b, 0)), DVec3::ZERO);
    let impulse = |w: &World| {
        let s = |i: usize| &w.bodies[i].structure;
        (s(a).velocity[0] * 2.0, s(b).velocity[0] * 50.0)
    };
    run(&mut world, &[strut], 1);
    let (pa, pb) = impulse(&world);
    assert!(
        (pa + pb).length() < 1e-9 * pa.length(),
        "momentum {pa} + {pb}"
    );
    assert!(pa.y > 0.0, "it pulled");
    assert!(pa.y <= 3600.0 * DT * (1.0 + 1e-6), "{} N·s in a step", pa.y);
    // Apart beyond its rest: no force at all.
    run(&mut world, &[strut], 60);
    let gap = world.bodies[a].structure.position[0].y - world.bodies[b].structure.position[0].y;
    assert!(gap > 1.0);
    let (pa2, _) = impulse(&world);
    run(&mut world, &[strut], 1);
    assert!(
        (impulse(&world).0 - pa2).length() < 1e-9,
        "it pulled apart bodies"
    );
}

#[test]
fn its_damper_damps_as_a_damped_oscillators() {
    // A 105 kg body on a strut, no gravity, pressed 1 cm and let go: ζ = c/(2·√(k·m)).
    let mut world = World::new(DT);
    let mut b = ball(DVec3::new(0.0, 0.99, 0.0), 105.0);
    b.structure.gravity = DVec3::ZERO;
    b.solver.reset_history(&b.structure);
    let a = world.add(b);
    let strut = Strut {
        damping: 0.0,
        ..leg(a, None, DVec3::ZERO)
    };
    let soft = Strut {
        damping: 0.3 * 2.0 * (30_000.0f64 * 105.0).sqrt(),
        ..strut
    };
    // It leaves the strut after half a period: the speed it leaves with tells the damping.
    let leaves = |strut: Strut| {
        let mut w = world.clone();
        run(&mut w, &[strut], 120);
        w.bodies[a].structure.velocity[0].y
    };
    let (undamped, damped) = (leaves(strut), leaves(soft));
    // Its law, one-sided (it lets go once k·u + c·u̇ reaches nought, before its rest: a leg
    // does not pull), integrated finely (RK4 at 10 µs).
    let reference = |c: f64| {
        let (k, m, most) = (30_000.0, 105.0, 3600.0);
        let accel = |x: f64, v: f64| (k * (1.0 - x) - c * v).clamp(0.0, most) / m;
        let (mut x, mut v, h) = (0.99, 0.0, 1e-5);
        while x < 1.0 {
            let (k1x, k1v) = (v, accel(x, v));
            let (k2x, k2v) = (
                v + 0.5 * h * k1v,
                accel(x + 0.5 * h * k1x, v + 0.5 * h * k1v),
            );
            let (k3x, k3v) = (
                v + 0.5 * h * k2v,
                accel(x + 0.5 * h * k2x, v + 0.5 * h * k2v),
            );
            let (k4x, k4v) = (v + h * k3v, accel(x + h * k3x, v + h * k3v));
            x += h / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
            v += h / 6.0 * (k1v + 2.0 * k2v + 2.0 * k3v + k4v);
        }
        v
    };
    // Undamped: all of ½k·x² back (the step is BDF2, its own damping a few per cent).
    let full = reference(0.0);
    assert!(
        (undamped - full).abs() < 0.06 * full,
        "{undamped} against {full}"
    );
    let (want, got) = (reference(soft.damping) / full, damped / undamped);
    assert!(
        (got - want).abs() < 0.03,
        "kept {got} of the speed, the law {want}"
    );
}
