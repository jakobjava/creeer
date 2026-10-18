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

/// (P1 exploration.) A ragdoll lying on the ground, a 200 kg crate dropped onto it: each
/// step's time and Newton iterations (the most of any of its bodies), and the totals.
#[test]
#[ignore = "exploration: a ragdoll under a crate"]
fn trace_a_crate_on_a_ragdoll() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let mut b = character::body(DVec3::new(0.0, 0.02, 0.0), DVec3::Y, DT);
    b.structure.angular_velocity[0] = DVec3::new(1.5, 0.0, 0.0);
    let c = world.add(b);
    let parts = fall(&mut world, c);
    for _ in 0..180 {
        world.step();
    }
    let at = world.bodies[parts[1]].structure.position[0] + DVec3::Y * 1.2;
    let mut crate_ = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        200.0,
        at,
        DT,
    );
    crate_.structure.gravity = DVec3::new(0.0, -9.81, 0.0);
    crate_.friction = 0.7;
    world.add(crate_);
    let (mut total, mut worst, mut its) = (0.0, 0.0f64, 0);
    for t in 0..240 {
        let start = std::time::Instant::now();
        world.step();
        let ms = start.elapsed().as_secs_f64() * 1e3;
        let most = world
            .bodies
            .iter()
            .map(|b| b.solver.last_iterations)
            .max()
            .unwrap_or(0);
        (total, worst, its) = (total + ms, worst.max(ms), its + most);
        if ms > 2.0 {
            eprintln!("{t}: {ms:.2} ms, {most} iterations");
        }
    }
    eprintln!("crate on a ragdoll: {total:.1} ms in all, worst {worst:.2} ms, {its} iterations");
}

/// (P1 exploration.) A standing character, a 200 kg crate dropped on its head from a metre:
/// knocked down, it falls under the crate. Each step's time, Newton iterations and substeps.
#[test]
#[ignore = "exploration: a crate on a character's head"]
fn trace_a_crate_on_a_head() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let c = world.add(character::body(DVec3::new(0.0, 0.0, 0.0), DVec3::Y, DT));
    let mut reactions = Vec::new();
    for _ in 0..60 {
        character::steer(
            &mut world,
            c,
            character::Intent::default(),
            (DVec3::Y, None),
            &mut reactions,
        );
        world.step();
        character::release(&mut world, &mut reactions);
    }
    let at = DVec3::new(0.15, character::HEIGHT + 1.5, 0.0);
    let mut crate_ = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        200.0,
        at,
        DT,
    );
    crate_.structure.gravity = world.bodies[c].structure.gravity;
    crate_.friction = 0.7;
    world.add(crate_);
    let (mut total, mut worst, mut down) = (0.0, 0.0f64, None);
    let mut phases = [0u64; creeer_physics::profile::SLOTS];
    let _ = creeer_physics::profile::take();
    for t in 0..300 {
        let before = world.bodies[c].structure.velocity[0];
        if down.is_none() {
            character::steer(
                &mut world,
                c,
                character::Intent::default(),
                (DVec3::Y, None),
                &mut reactions,
            );
        }
        let start = std::time::Instant::now();
        world.step();
        let ms = start.elapsed().as_secs_f64() * 1e3;
        let pushes = world.strut_pushes();
        character::release(&mut world, &mut reactions);
        if down.is_none()
            && knocked(
                &world,
                c,
                DVec3::Y,
                (before, super::super::injury::pushed_on(&pushes, c)),
            )
        {
            down = Some(t);
            fall(&mut world, c);
        }
        let most = world
            .bodies
            .iter()
            .map(|b| b.solver.last_iterations)
            .max()
            .unwrap_or(0);
        let spent = creeer_physics::profile::take();
        let steps = spent[creeer_physics::profile::Slot::WorldSteps as usize];
        if (130..170).contains(&t) {
            for (p, s) in phases.iter_mut().zip(spent) {
                *p += s;
            }
        }
        (total, worst) = (total + ms, worst.max(ms));
        if ms > 2.0 {
            eprintln!("{t}: {ms:.2} ms, {most} iterations (last substep), {steps} world steps");
        }
    }
    let list: Vec<String> = (creeer_physics::profile::NAMES.iter())
        .filter(|(s, _)| phases[*s as usize] > 0)
        .map(|(s, n)| format!("{n} {}", phases[*s as usize]))
        .collect();
    eprintln!("ticks 130..170: {}", list.join(", "));
    eprintln!("crate on a head: down at {down:?}, {total:.1} ms in all, worst {worst:.2} ms");
}
