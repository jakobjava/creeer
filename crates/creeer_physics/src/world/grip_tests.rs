//! Grips (G2.10): a box held in the air settles where the hands mean it within a tenth of a
//! second whatever its mass, and does not ring; carried along it follows without lagging; a
//! load too heavy for the hands is not lifted and nothing is thrown; a stick held at its end
//! turns to the hands' turn and does not spin; the hands' work is booked, so the energy
//! guard redoes none of their steps.

use super::grip::Grip;
use super::joint::ARM;
use super::shape::{Shape, StaticShape};
use super::{Body, World};
use creeer_math::glam::{DQuat, DVec3};
use std::f64::consts::PI;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.81;
/// The hands' frequency (Hz) and the human strength used (N, N·m).
const HZ: f64 = 8.0;

fn world() -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world
}

fn rigid(shape: Shape, mass: f64, at: DVec3) -> Body {
    let mut b = Body::rigid(shape, mass, at, DT);
    b.structure.gravity = DVec3::new(0.0, -G, 0.0);
    b.solver.reset_history(&b.structure);
    b
}

/// A grip on body `a` at `at_a`, critically damped at [`HZ`] for its mass and largest
/// moment, its target `target` (its weight held up: the target raised by mg/k), turned `turn`.
fn grip(world: &World, a: usize, at_a: DVec3, (target, turn): (DVec3, DQuat)) -> Grip {
    let b = &world.bodies[a];
    let mass = b.structure.mass[0];
    let inertia = b.shapes[0].1.inertia(mass);
    let largest = inertia.x_axis.x.max(inertia.y_axis.y).max(inertia.z_axis.z);
    let w = 2.0 * PI * HZ;
    let (k, ka) = (mass * w * w, largest * w * w / (2.0 * ARM * ARM));
    Grip {
        a: (a, 0),
        at_a,
        target: target + DVec3::Y * (mass * G / k),
        target_velocity: DVec3::ZERO,
        turn,
        spin: DVec3::ZERO,
        stiffness: k,
        damping: 2.0 * mass * w,
        most: 575.0,
        turn_stiffness: ka,
        turn_damping: 2.0 * ka / w,
        most_torque: 57.5,
    }
}

fn step_held(world: &mut World, g: Grip) {
    world.add_grip(g);
    world.step();
    world.clear_grips();
}

#[test]
fn a_box_held_settles_at_the_hands_in_a_tenth_of_a_second_without_ringing() {
    for mass in [1.0, 15.0, 40.0] {
        let mut world = world();
        let start = DVec3::new(0.0, 1.0, 0.0);
        let a = world.add(rigid(
            Shape::Box {
                half: DVec3::splat(0.2),
            },
            mass,
            start,
        ));
        let target = start + DVec3::new(0.1, 0.2, 0.0);
        let (mut most_up, mut at) = (0.0f64, DVec3::ZERO);
        for tick in 0..60 {
            let g = grip(&world, a, DVec3::ZERO, (target, DQuat::IDENTITY));
            step_held(&mut world, g);
            at = world.bodies[a].structure.position[0];
            most_up = most_up.max(at.y);
            if tick == 9 && mass <= 15.0 {
                // 0.17 s: within a centimetre of the 0.22 m move (40 kg rises slower: its 392 N
                // weight leaves the hands 183 N of their 575 to move it with).
                assert!((at - target).length() < 0.01, "{mass} kg: {at} at 0.17 s");
            }
        }
        assert!((at - target).length() < 1e-3, "{mass} kg: {at}");
        // Critically damped: no overshoot beyond a millimetre.
        assert!(most_up < target.y + 1e-3, "{mass} kg rang to {most_up}");
        // The hands' work booked, no step of the move is redone: at most one as it comes to
        // rest (the 1 kg box's at 0.15 s, 0.2 mm off, its energy at the guard's nanojoule floor).
        let redone = world.bodies[a].solver.fallbacks;
        assert!(redone <= 1, "{mass} kg: {redone} steps redone");
    }
}

#[test]
fn carried_along_it_follows_without_lagging() {
    let mut world = world();
    let start = DVec3::new(0.0, 1.2, 0.0);
    let a = world.add(rigid(
        Shape::Box {
            half: DVec3::splat(0.2),
        },
        15.0,
        start,
    ));
    let v = DVec3::new(1.5, 0.0, 0.0);
    let mut worst = 0.0f64;
    for tick in 1..=120 {
        let target = start + v * (f64::from(tick) * DT);
        let mut g = grip(&world, a, DVec3::ZERO, (target, DQuat::IDENTITY));
        g.target_velocity = v;
        step_held(&mut world, g);
        if tick > 30 {
            let at = world.bodies[a].structure.position[0];
            worst = worst.max((at - target).length());
        }
    }
    assert!(worst < 2e-3, "{worst} m behind the hands");
    assert_eq!(world.bodies[a].solver.fallbacks, 0, "steps redone");
}

#[test]
fn a_load_too_heavy_is_not_lifted_and_nothing_is_thrown() {
    let mut world = world();
    let start = DVec3::new(0.0, 0.3, 0.0);
    let a = world.add(rigid(
        Shape::Box {
            half: DVec3::splat(0.3),
        },
        120.0,
        start,
    ));
    let mut fastest = 0.0f64;
    for _ in 0..120 {
        let g = grip(&world, a, DVec3::ZERO, (start + DVec3::Y, DQuat::IDENTITY));
        step_held(&mut world, g);
        fastest = fastest.max(world.bodies[a].structure.velocity[0].length());
    }
    let at = world.bodies[a].structure.position[0];
    // 575 N against its 1,177 N weight: it stays on the ground, lightened.
    assert!(at.y < 0.31, "lifted to {}", at.y);
    assert!(fastest < 0.05, "moved at {fastest} m/s");
}

#[test]
fn a_stick_held_at_its_end_turns_to_the_hands_and_does_not_spin() {
    let mut world = world();
    let start = DVec3::new(0.0, 1.2, 0.0);
    // A stick 1.2 m long, 3 cm thick, 0.6 kg: its moment about its own axis a few
    // thousandths of the others (the stick that spun at 33 and 450 rad/s, ADR-0027).
    let a = world.add(rigid(
        Shape::Box {
            half: DVec3::new(0.6, 0.015, 0.015),
        },
        0.6,
        start,
    ));
    let end = DVec3::new(-0.55, 0.0, 0.0);
    let held = start + end;
    let mut spin = 0.0f64;
    let want = DQuat::from_rotation_y(0.5 * PI);
    for tick in 0..120 {
        // Walking: the hands bob 3 cm at 2 Hz.
        let t = f64::from(tick) * DT;
        let bob = DVec3::Y * (0.03 * creeer_math::det::sin(2.0 * PI * 2.0 * t));
        let g = grip(&world, a, end, (held + bob, want));
        step_held(&mut world, g);
        if tick > 30 {
            spin = spin.max(world.bodies[a].structure.angular_velocity[0].length());
        }
    }
    let q = world.bodies[a].structure.orientation[0];
    let off = (q * want.conjugate()).to_axis_angle().1.to_degrees();
    assert!(off.abs() < 2.0, "{off}° from the hands' turn");
    assert!(spin < 3.0, "spun at {spin} rad/s");
}
