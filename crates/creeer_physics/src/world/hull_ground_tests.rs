//! Hulls on the ground (`ground_hull.rs`): as boxes on flat ground and over a bump, and a
//! rock coming to rest on bumpy ground.

use super::ground;
use super::hull::Hull;
use super::hull_contact_tests::{numbers, same, summary};
use super::narrow::pose;
use super::planet_tests::FieldGround;
use super::shape::{RawContact, Shape};
use super::*;
use creeer_math::det;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;

/// On flat ground a hull built from a box's corners touches as the box: the same patches,
/// in 500 random poses from 1 cm above the ground to 1 cm in.
#[test]
fn a_hull_of_a_box_touches_the_ground_as_the_box() {
    let flat = FieldGround(|p: DVec3| p.y);
    let half = DVec3::new(0.5, 0.3, 0.2);
    let (hull, as_box) = (
        Shape::Hull(Arc::new(Hull::cuboid(half))),
        Shape::Box { half },
    );
    let mut next = numbers(3);
    let mut touching = 0;
    for _ in 0..500 {
        let q = rotation(
            DVec3::new(next() - 0.5, next() - 0.5, next() - 0.5),
            6.0 * next(),
        );
        // Its lowest corner from 1 cm above to 1 cm below the ground.
        let r = creeer_math::glam::DMat3::from_quat(q);
        let reach = (0..3).map(|k| half[k] * r.col(k).y.abs()).sum::<f64>();
        let at = pose(DVec3::new(next(), reach + 0.02 * (next() - 0.5), next()), q);
        let boxes = ground::contacts(&flat, at, &as_box, 0.005);
        let hulls = ground::contacts(&flat, at, &hull, 0.005);
        assert!(
            same(&summary(&boxes), &summary(&hulls)),
            "{at:?}\n{boxes:?}\n{hulls:?}"
        );
        touching += usize::from(!boxes.is_empty());
    }
    assert!(touching > 100 && touching < 450, "{touching} touching");
}

/// A 2 m square slab lying 12 mm over flat ground with a 1 cm bump under it finds the bump
/// as the box does: its corners beyond the margin, its bottom face presses as a patch that
/// measures the ground over the bump (`ground_rise.rs`), the bump's top 2 mm under it to
/// 1.5 mm (3.2 mm measured: its triangles' corners, 9 cm apart, measure the ground, and the
/// nearest is off the top), with the same contacts as the box's.
#[test]
fn a_slab_over_a_bump_finds_it_as_a_box_does() {
    let bump = FieldGround(|p: DVec3| p.y - 0.01 * det::exp(-(p.x * p.x + p.z * p.z) / 0.02));
    let half = DVec3::new(1.0, 0.1, 1.0);
    let at = pose(DVec3::new(0.05, 0.1 + 0.012, 0.0), DQuat::IDENTITY);
    let boxes = ground::contacts(&bump, at, &Shape::Box { half }, 0.005);
    let hulls = ground::contacts(&bump, at, &Shape::Hull(Arc::new(Hull::cuboid(half))), 0.005);
    assert!(!boxes.is_empty(), "the box finds the bump");
    let points = |c: &[RawContact]| {
        let mut p: Vec<[f64; 3]> = c.iter().map(|c| c.point_a.to_array()).collect();
        p.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        p
    };
    assert_eq!(points(&boxes), points(&hulls), "the same corners");
    // The least gap its patch measures: over the bump's top.
    let sampled = boxes[0]
        .patch
        .as_ref()
        .and_then(|v| v.domain.sampled())
        .expect("measured");
    let corners: Vec<DVec3> = boxes.iter().take(4).map(|c| c.point_a).collect();
    let least = sampled
        .vertices
        .iter()
        .zip(&sampled.feet)
        .map(|(&[s, t], &(foot, n))| {
            let p = corners[0] + (corners[1] - corners[0]) * s + (corners[3] - corners[0]) * t;
            n.dot(p - foot)
        })
        .fold(f64::INFINITY, f64::min);
    assert!((least - 0.002).abs() < 1.5e-3, "least gap {least} m");
}

/// A rock dropped on bumpy ground (10 cm bumps 2 m apart) comes to rest and falls asleep
/// within 6 s (2.3 s measured) without gaining energy, no corner more than 2 mm in (as on
/// flat ground, `hull_contact_tests.rs`). (It rocked at 0.1 m/s on the points under its
/// faces counted as bumps of their own; with one quadric for the ground under each face it
/// was kicked every few steps, then turned at a milliradian a second on a bump for seconds,
/// F5.)
#[test]
fn a_rock_comes_to_rest_on_bumpy_ground() {
    let bumps = |p: DVec3| {
        p.y - 0.1 * det::sin(p.x * std::f64::consts::PI) * det::sin(p.z * std::f64::consts::PI)
    };
    let mut world = World::new(DT);
    world.ground = Some(Arc::new(FieldGround(bumps)));
    world.static_friction = 0.7;
    let flat: Vec<DVec3> = super::hull_tests::cloud(10, 0.3, 9)
        .into_iter()
        .map(|p| p * DVec3::new(1.0, 0.7, 0.45))
        .collect();
    let (hull, _) = Hull::new(&flat).expect("a rock");
    let hull = Arc::new(hull);
    let mass = 2600.0 * hull.volume();
    let mut rock = Body::rigid(
        Shape::Hull(hull.clone()),
        mass,
        DVec3::new(0.3, 1.0, 0.2),
        DT,
    );
    rock.structure.orientation[0] = rotation(DVec3::new(1.0, 2.0, 0.5), 1.0);
    rock.surface = surface::Surface::ROCK;
    rock.friction = 0.7;
    world.add(rock);
    let start = world.energy();
    let released = mass * 9.806_65 * 1.0;
    for tick in 0..360 {
        world.step();
        let now = world.energy();
        assert!(
            now <= start + 1e-3 * released,
            "tick {tick}: {now} > {start}"
        );
    }
    let s = &world.bodies[0].structure;
    assert!(
        world.bodies[0].sleep.asleep,
        "still moving at {}",
        s.velocity[0]
    );
    for &v in hull.vertices() {
        let p = s.position[0] + s.orientation[0] * v;
        assert!(bumps(p) > -2e-3, "a corner {} m in", -bumps(p));
    }
}

/// A box at rest on the ground stays put: on a 10° slope of flat ground, on the 17° flank of
/// a ridge, dropped onto bumps, and set down in a hollow between bumps on its 12° flank (two
/// of its sides pressing too), it moves less than 10 µm over two seconds and falls asleep.
/// (It crept at a millimetre a second down the bumps while the forces of lifted patch corners
/// were missing and went undamped; in the hollow, its faces measured against the ground
/// under their centroids gave a corner two friction planes 14° apart, and it crept 2 cm in
/// 5 s, F5.)
#[test]
fn a_box_at_rest_on_a_ground_slope_never_creeps() {
    let (sin, cos) = det::sin_cos(10f64.to_radians());
    let normal = DVec3::new(-sin, cos, 0.0);
    let pi = std::f64::consts::PI;
    for case in 0..4 {
        let mut world = World::new(DT);
        world.static_friction = 0.7;
        let half = DVec3::new(0.3, 0.2, 0.15);
        let mut b = Body::rigid(Shape::Box { half }, 50.0, normal * 0.2, DT);
        b.structure.orientation[0] = rotation(DVec3::Z, 10f64.to_radians());
        match case {
            0 => world.ground = Some(Arc::new(FieldGround(move |p: DVec3| p.dot(normal)))),
            1 => {
                world.ground = Some(Arc::new(FieldGround(|p: DVec3| p.y - 0.3 * det::sin(p.x))));
                // On the flank at x = 0: slope 0.3.
                let (s, c) = (0.3 / 1.09f64.sqrt(), 1.0 / 1.09f64.sqrt());
                b.structure.position[0] = DVec3::new(-s, c, 0.0) * 0.2;
                b.structure.orientation[0] = rotation(DVec3::Z, det::atan2(s, c));
            }
            _ => {
                let bumps = move |p: DVec3| p.y - 0.1 * det::sin(p.x * pi) * det::sin(p.z * pi);
                world.ground = Some(Arc::new(FieldGround(bumps)));
                if case == 2 {
                    b.structure.position[0] = DVec3::new(0.3, 1.0, 0.2);
                    b.structure.orientation[0] = rotation(DVec3::new(1.0, 2.0, 0.5), 1.0);
                } else {
                    // Its bottom on the hollow's flank at (0.25, −0.25), along the ground's normal.
                    let at = DVec3::new(0.25, -0.05, -0.25);
                    let (foot, n) = ground::project(&FieldGround(bumps), at).expect("ground");
                    b.structure.position[0] = foot + n * (half.y + 0.002);
                    b.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::Y, n);
                }
            }
        }
        b.friction = 0.7;
        world.add(b);
        for _ in 0..360 {
            world.step();
        }
        let start = world.bodies[0].structure.position[0];
        for _ in 0..120 {
            world.step();
        }
        let moved = (world.bodies[0].structure.position[0] - start).length();
        assert!(moved < 1e-5, "case {case}: moved {moved} m");
        assert!(world.bodies[0].sleep.asleep, "case {case}: not asleep");
    }
}
