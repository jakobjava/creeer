//! The jetpack against the rocket equation: a full tank burnt in free fall gives
//! Tsiolkovsky's Δv = Isp·g₀·ln(m₀/m₁); a hover lasts as long as its gas at the flow its
//! wearer's weight needs; the attitude jets damp a spin.

use super::*;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;

/// Steps the character `n` times asking `intent`, with ground or not.
fn run(world: &mut World, c: usize, n: usize, intent: Intent) {
    let mut reactions = Vec::new();
    for _ in 0..n {
        steer(world, c, intent, (DVec3::Y, None), &mut reactions);
        world.step();
        release(world, &mut reactions);
    }
}

#[test]
fn a_full_tank_burnt_in_free_fall_gives_tsiolkovskys_delta_v() {
    let mut world = World::new(DT);
    let mut b = body(DVec3::ZERO, DVec3::Y, DT);
    b.structure.gravity = DVec3::ZERO;
    let c = world.add(b);
    let push = Intent {
        thrust: DVec3::X,
        ..Intent::default()
    };
    // Full thrust: 1.5 kN, 2.2 kg/s, the tank empty after 1.4 s.
    run(&mut world, c, 150, push);
    assert!(
        gas(&world.bodies[c]) < 1e-12,
        "{} kg left",
        gas(&world.bodies[c])
    );
    assert!((world.bodies[c].structure.masses()[0] - DRY).abs() < 1e-12);
    let v = world.bodies[c].structure.velocity[0];
    let want = ISP * G * creeer_math::det::ln(MASS / DRY);
    // (Stepped as a free body, Newmark: its impulses exact. BDF2 fell 0.6 % short.)
    assert!(
        (v.x / want - 1.0).abs() < 1e-5,
        "Δv {} m/s against {want}",
        v.x
    );
    assert!(v.y.abs() < 1e-9 && v.z.abs() < 1e-9);
    // Empty, it pushes no more.
    run(&mut world, c, 30, push);
    assert!((world.bodies[c].structure.velocity[0].x - v.x).abs() < 1e-9);
}

#[test]
fn a_hover_lasts_as_long_as_its_gas() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let c = world.add(body(DVec3::ZERO, DVec3::Y, DT));
    run(&mut world, c, 60, Intent::default());
    // Its weight's thrust: the flow m·g/(Isp·g₀) lasts gas/flow (about 2 s).
    let hover = Intent {
        thrust: DVec3::Y * (MASS * G / THRUST_MOST),
        ..Intent::default()
    };
    let start = world.bodies[c].structure.position[0].y;
    let mut ticks = 0;
    while gas(&world.bodies[c]) > 0.0 && ticks < 600 {
        run(&mut world, c, 1, hover);
        ticks += 1;
    }
    let lasted = f64::from(ticks) * DT;
    let want = TANK * ISP / MASS;
    assert!(
        (lasted / want - 1.0).abs() < 0.05,
        "lasted {lasted} s, {want} s"
    );
    // It held itself up (lighter as it burnt, it rose a little; the legs left the ground).
    let rise = world.bodies[c].structure.position[0].y - start;
    assert!(rise > 0.0 && rise < 2.0, "rose {rise} m");
}

#[test]
fn the_attitude_jets_damp_a_spin() {
    let mut world = World::new(DT);
    let mut b = body(DVec3::ZERO, DVec3::Y, DT);
    b.structure.gravity = DVec3::ZERO;
    b.structure.angular_velocity[0] = DVec3::new(0.3, 1.0, -0.2);
    let c = world.add(b);
    let spin = world.bodies[c].structure.angular_velocity[0].length();
    run(
        &mut world,
        c,
        120,
        Intent {
            thrust: DVec3::X * 0.01,
            ..Intent::default()
        },
    );
    let after = world.bodies[c].structure.angular_velocity[0].length();
    assert!(after < 0.2 * spin, "spins at {after} rad/s of {spin}");
}
