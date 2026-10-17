//! TEMP: the Hessian check (feature `profile`, CREEER_CHECK_HESSIAN=1) in one kind of
//! contact at a time.

use super::shape::{Shape, StaticShape};
use super::*;
use creeer_math::glam::{DQuat, DVec3};

const DT: f64 = 1.0 / 60.0;

fn ground(slope: f64) -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DQuat::from_rotation_z(slope.to_radians()) * DVec3::Y,
        offset: 0.0,
    });
    world
}

fn run(name: &str, mut world: World, ticks: usize) {
    eprintln!("=== {name}");
    for _ in 0..ticks {
        world.step();
    }
}

#[test]
#[ignore = "probe"]
fn probe_hessian_scenes() {
    let cube = Shape::Box {
        half: DVec3::splat(0.5),
    };
    // A ball resting (Hertz, no slip).
    let mut w = ground(0.0);
    w.add(Body::rigid(
        Shape::Sphere { radius: 0.5 },
        100.0,
        DVec3::new(0.0, 0.5, 0.0),
        DT,
    ));
    run("ball resting", w, 30);
    // A box resting flat (a face patch).
    let mut w = ground(0.0);
    w.add(Body::rigid(
        cube.clone(),
        100.0,
        DVec3::new(0.0, 0.5, 0.0),
        DT,
    ));
    run("box resting", w, 30);
    // A box sliding down 40° (patch and sliding friction).
    let mut w = ground(40.0);
    let mut b = Body::rigid(cube.clone(), 100.0, DVec3::new(0.0, 0.5, 0.0), DT);
    b.structure.orientation[0] = DQuat::from_rotation_z(40f64.to_radians());
    b.friction = 0.3;
    w.add(b);
    run("box sliding", w, 30);
    // A box on another box (a face between two bodies).
    let mut w = ground(0.0);
    w.add(Body::rigid(
        cube.clone(),
        100.0,
        DVec3::new(0.0, 0.5, 0.0),
        DT,
    ));
    w.add(Body::rigid(
        cube.clone(),
        100.0,
        DVec3::new(0.2, 1.5, 0.1),
        DT,
    ));
    run("box on box", w, 30);
    // A box landing on an edge, tumbling.
    let mut w = ground(0.0);
    let mut b = Body::rigid(cube, 100.0, DVec3::new(0.0, 1.5, 0.0), DT);
    b.structure.orientation[0] = DQuat::from_rotation_z(0.6) * DQuat::from_rotation_x(0.3);
    w.add(b);
    run("box tumbling", w, 90);
    let rock = || {
        let points = [
            DVec3::new(0.3, 0.0, 0.02),
            DVec3::new(-0.28, 0.03, 0.0),
            DVec3::new(0.01, 0.2, -0.03),
            DVec3::new(0.02, -0.21, 0.01),
            DVec3::new(-0.02, 0.01, 0.25),
            DVec3::new(0.03, -0.02, -0.24),
            DVec3::new(0.15, 0.14, 0.1),
            DVec3::new(-0.12, 0.13, -0.14),
            DVec3::new(0.11, -0.15, 0.13),
        ];
        std::sync::Arc::new(super::hull::Hull::new(&points).expect("a hull").0)
    };
    // A rock on flat ground, and sliding on a 35° ramp (a static box), as in the pile.
    let mut w = ground(0.0);
    w.add(Body::rigid(
        Shape::Hull(rock()),
        50.0,
        DVec3::new(0.0, 0.4, 0.0),
        DT,
    ));
    run("rock resting", w, 60);
    let mut w = World::new(DT);
    w.statics.push(StaticShape::Box {
        center: DVec3::ZERO,
        rotation: DQuat::from_rotation_z(-35f64.to_radians()),
        half: DVec3::new(3.0, 0.2, 2.0),
    });
    let mut b = Body::rigid(Shape::Hull(rock()), 50.0, DVec3::new(0.0, 0.7, 0.0), DT);
    b.friction = 0.6;
    b.surface = super::surface::Surface::ROCK;
    w.add(b);
    run("rock on ramp", w, 90);
}

#[test]
#[ignore = "probe"]
fn probe_hessian_pile() {
    let friction: f64 = std::env::var("PILE_FRICTION")
        .ok()
        .and_then(|f| f.parse().ok())
        .unwrap_or(0.6);
    let points = [
        DVec3::new(0.3, 0.0, 0.02),
        DVec3::new(-0.28, 0.03, 0.0),
        DVec3::new(0.01, 0.2, -0.03),
        DVec3::new(0.02, -0.21, 0.01),
        DVec3::new(-0.02, 0.01, 0.25),
        DVec3::new(0.03, -0.02, -0.24),
        DVec3::new(0.15, 0.14, 0.1),
        DVec3::new(-0.12, 0.13, -0.14),
        DVec3::new(0.11, -0.15, 0.13),
    ];
    let hull = std::sync::Arc::new(super::hull::Hull::new(&points).expect("a hull").0);
    let mut w = World::new(DT);
    w.static_friction = friction;
    w.statics.push(StaticShape::Box {
        center: DVec3::ZERO,
        rotation: DQuat::from_rotation_z(-35f64.to_radians()),
        half: DVec3::new(3.0, 0.2, 2.0),
    });
    w.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: -2.0,
    });
    for k in 0..10 {
        let at = DVec3::new(
            -1.0 + 0.4 * f64::from(k % 5),
            1.5 + 0.6 * f64::from(k / 5),
            -0.4 + 0.2 * f64::from(k % 3),
        );
        let mut b = Body::rigid(Shape::Hull(hull.clone()), 50.0, at, DT);
        b.structure.orientation[0] = DQuat::from_axis_angle(
            DVec3::new(1.0, f64::from(k), 0.5).normalize(),
            0.7 * f64::from(k),
        );
        b.friction = friction;
        b.surface = super::surface::Surface::ROCK;
        w.add(b);
    }
    run("pile", w, 240);
}
