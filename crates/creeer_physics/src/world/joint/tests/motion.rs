//! Joints in motion: a double pendulum against an RK4 integration of its own two angles (the
//! reduced coordinates, ADR-0022's reference), a damped pendulum's decay against its closed
//! form, a drive holding its target against a load below its strength and giving way to
//! one beyond it, and friction holding a load below its torque.

use super::{DT, G, angle, ball, ball_joint};
use crate::world::World;
use crate::world::joint::{Drive, Joint, Kind};
use creeer_math::glam::{DQuat, DVec3};

/// A planar double pendulum of two balls (mass 1 kg, radius 5 cm, inertia 2/5·m·r² about
/// their centres), the first `l1` below the pivot, the second `l2` below the first: θ̈ from
/// its Lagrangian (the masses' and the balls' own turn).
fn accelerations(t1: f64, t2: f64, w1: f64, w2: f64, l1: f64, l2: f64) -> (f64, f64) {
    let i = 0.4 * 0.05 * 0.05;
    let (m11, m22) = (2.0 * l1 * l1 + i, l2 * l2 + i);
    let m12 = l1 * l2 * creeer_math::det::cos(t1 - t2);
    let s = creeer_math::det::sin(t1 - t2);
    let f1 = -l1 * l2 * s * w2 * w2 - 2.0 * G * l1 * creeer_math::det::sin(t1);
    let f2 = l1 * l2 * s * w1 * w1 - G * l2 * creeer_math::det::sin(t2);
    let det = m11 * m22 - m12 * m12;
    ((m22 * f1 - m12 * f2) / det, (m11 * f2 - m12 * f1) / det)
}

/// RK4 of the double pendulum's angles from rest, sampled every `dt` for `steps` steps.
fn reference((t1, t2): (f64, f64), (l1, l2): (f64, f64), dt: f64, steps: usize) -> Vec<(f64, f64)> {
    let fine = 100;
    let h = dt / f64::from(fine);
    let mut y = [t1, t2, 0.0, 0.0];
    let f = |y: [f64; 4]| {
        let (a1, a2) = accelerations(y[0], y[1], y[2], y[3], l1, l2);
        [y[2], y[3], a1, a2]
    };
    let add = |y: [f64; 4], k: [f64; 4], s: f64| std::array::from_fn(|i| y[i] + s * k[i]);
    let mut out = Vec::with_capacity(steps);
    for _ in 0..steps {
        for _ in 0..fine {
            let k1 = f(y);
            let k2 = f(add(y, k1, 0.5 * h));
            let k3 = f(add(y, k2, 0.5 * h));
            let k4 = f(add(y, k3, h));
            y = std::array::from_fn(|i| {
                y[i] + h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i])
            });
        }
        out.push((y[0], y[1]));
    }
    out
}

#[test]
fn a_double_pendulum_follows_its_reduced_coordinates() {
    // At 240 Hz (Newmark's own phase error then below the comparison's), and at 60 Hz.
    for (dt, tolerance) in [(1.0 / 240.0, 2e-3), (DT, 2e-2)] {
        let (l1, l2) = (0.5, 0.5);
        let (t1, t2): (f64, f64) = (30f64.to_radians(), -20f64.to_radians());
        let mut world = World::new(dt);
        let p1 = DVec3::new(
            l1 * creeer_math::det::sin(t1),
            -l1 * creeer_math::det::cos(t1),
            0.0,
        );
        let p2 = p1
            + DVec3::new(
                l2 * creeer_math::det::sin(t2),
                -l2 * creeer_math::det::cos(t2),
                0.0,
            );
        let (a, b) = (ball(&mut world, p1, 1.0), ball(&mut world, p2, 1.0));
        ball_joint(&mut world, a, None, DVec3::ZERO);
        // The second hangs from the first's centre.
        ball_joint(&mut world, b, Some(a), p1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (3.0 / dt).round() as usize;
        let want = reference((t1, t2), (l1, l2), dt, steps);
        let mut worst = 0.0f64;
        for (t1, t2) in want {
            world.step();
            let (x1, x2) = (
                world.bodies[a].structure.position[0],
                world.bodies[b].structure.position[0],
            );
            worst = worst
                .max((angle(DVec3::ZERO, x1) - t1).abs())
                .max((angle(x1, x2) - t2).abs());
        }
        assert!(
            worst < tolerance,
            "at {dt} s: {worst} rad off the reference"
        );
    }
}

/// A hinge about z at the origin holding a 1 kg ball 1 m out along x.
fn hinged(world: &mut World) -> (usize, usize) {
    let bob = ball(world, DVec3::new(1.0, 0.0, 0.0), 1.0);
    let joint = Joint::new(
        world,
        ((bob, 0), None),
        (DVec3::ZERO, DVec3::Z, DVec3::X),
        Kind::Hinge { range: None },
    );
    (bob, world.join(joint))
}

/// How far a hinged ball has turned down from x (rad).
fn turned(world: &World, bob: usize) -> f64 {
    let at = world.bodies[bob].structure.position[0];
    creeer_math::det::atan2(-at.y, at.x)
}

#[test]
fn a_damped_pendulum_decays_as_its_closed_form() {
    let mut world = World::new(DT);
    let (bob, j) = hinged(&mut world);
    world.joint_mut(j).damping = 0.2;
    // Swinging about straight down (−π/2 from x), small: I·θ̈ + c·θ̇ + m·g·L·θ = 0, its
    // amplitude falling as e^(−c·t/2I).
    let inertia = 1.0 + 0.4 * 0.05 * 0.05;
    let start = std::f64::consts::FRAC_PI_2 + 0.05;
    world.bodies[bob].structure.position[0] = DVec3::new(
        creeer_math::det::cos(start),
        -creeer_math::det::sin(start),
        0.0,
    );
    world.bodies[bob].structure.orientation[0] = DQuat::from_rotation_z(-start);
    let body = &mut world.bodies[bob];
    body.solver.reset_history(&body.structure);
    let mut peaks = Vec::new();
    let (mut before, mut last) = (0.0, turned(&world, bob) - std::f64::consts::FRAC_PI_2);
    for step in 0..600 {
        world.step();
        let now = turned(&world, bob) - std::f64::consts::FRAC_PI_2;
        if step > 1 && last > before && last > now {
            peaks.push((f64::from(step) * DT, last));
        }
        (before, last) = (last, now);
    }
    let (first, end) = (peaks[0], peaks[peaks.len() - 1]);
    let rate = creeer_math::det::ln(first.1 / end.1) / (end.0 - first.0);
    let want = 0.2 / (2.0 * inertia);
    assert!(
        (rate / want - 1.0).abs() < 0.03,
        "decays at {rate}/s against {want}"
    );
}

#[test]
fn a_drive_holds_its_target_within_its_strength() {
    // Its target: the ball turned 0.4 rad down from x (B, the world, turned from A's frame
    // the other way about the axis).
    for (strength, holds) in [(50.0, true), (5.0, false)] {
        let mut world = World::new(DT);
        let (bob, j) = hinged(&mut world);
        let joint = world.joint_mut(j);
        joint.damping = 5.0;
        joint.drive = Some(Drive {
            target: DQuat::from_rotation_x(0.4),
            stiffness: 1000.0,
            strength,
        });
        for _ in 0..300 {
            world.step();
        }
        let at = turned(&world, bob);
        // Within its strength the load's 9.8 N·m bends it by about 9.8/1000 rad.
        let off = (at - 0.4).abs();
        if holds {
            assert!(off < 0.02, "held at {at} rad");
        } else {
            assert!(at > 1.0, "a 5 N·m drive held the 9.8 N·m load at {at} rad");
        }
    }
}

#[test]
fn friction_holds_a_load_below_its_torque() {
    for (friction, holds) in [(20.0, true), (5.0, false)] {
        let mut world = World::new(DT);
        let (bob, j) = hinged(&mut world);
        world.joint_mut(j).friction = friction;
        for _ in 0..120 {
            world.step();
        }
        let at = turned(&world, bob);
        if holds {
            assert!(
                at.abs() < 1e-3,
                "a {friction} N·m joint let it turn {at} rad"
            );
        } else {
            assert!(at > 0.5, "a {friction} N·m joint held it at {at} rad");
        }
    }
}

#[test]
fn a_ball_lifted_at_a_steady_speed_is_held_with_its_weight() {
    // 5 kg hung from the world, its point drawn up 1 cm a step (0.6 m/s) for two seconds: the
    // force the joint applies is the ball's weight (it no longer speeds up), though its
    // multiplier alone, its refreshes run out each step, is not.
    let mut world = World::new(DT);
    let b = ball(&mut world, DVec3::new(0.0, -0.2, 0.0), 5.0);
    let j = ball_joint(&mut world, b, None, DVec3::ZERO);
    for _ in 0..120 {
        let joint = world.joint_mut(j);
        joint.at_b += DVec3::Y * 0.01;
        joint.moving[1] = DVec3::Y * 0.6;
        world.step();
    }
    let weight = 5.0 * G;
    let held = world.joints()[j].force();
    assert!(
        (held - DVec3::Y * weight).length() < 0.02 * weight,
        "the joint holds {held} N of {weight}"
    );
    let v = world.bodies[b].structure.velocity[0];
    assert!((v - DVec3::Y * 0.6).length() < 1e-3, "rising at {v}");
}
