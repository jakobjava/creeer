//! The feet on a body (`feet.rs`): the balance's torque that passes through them, and the
//! patch's geometry.

use super::feet::{carried, clip, mean_distance, nearest, octagon};
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_physics::world::hull::Hull;
use creeer_physics::world::ray::Hit;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use std::sync::Arc;

fn square(h: f64) -> Vec<[f64; 2]> {
    vec![[-h, -h], [h, -h], [h, h], [-h, h]]
}

#[test]
fn a_squares_mean_distance_from_its_centre_is_its_own() {
    // ∫|r| over a square of side 2h from its centre: h·(√2 + asinh 1)/3·… = 0.7652·h.
    let m = mean_distance(&square(1.0), [0.0, 0.0]);
    assert!((m - 0.765_196).abs() < 1e-5, "{m}");
    // From a corner: the same square seen from there, twice as far on the whole.
    let corner = mean_distance(&square(0.5), [-0.5, -0.5]);
    assert!((corner - 0.765_196).abs() < 1e-5, "{corner}");
    // Clockwise, the same.
    let mut cw = square(1.0);
    cw.reverse();
    assert!((mean_distance(&cw, [0.0, 0.0]) - m).abs() < 1e-12);
}

#[test]
fn a_strip_is_clipped_to_the_feet_and_its_centre_kept_within() {
    // A pole's top face, 9.75 mm across, 1.6 m long: within the feet's reach, 0.266 m.
    let strip = vec![
        [-0.8, -0.004_875],
        [0.8, -0.004_875],
        [0.8, 0.004_875],
        [-0.8, 0.004_875],
    ];
    let patch = clip(&strip, &octagon(0.133));
    // (The octagon's sides face along the axes: the strip ends at its inner radius.)
    let xs = patch.iter().map(|p| p[0].abs()).fold(0.0, f64::max);
    assert!((xs - 0.133).abs() < 1e-9, "{xs}");
    let [x, y] = nearest(&patch, [0.01, 0.02]);
    assert!((x - 0.01).abs() < 1e-12 && (y - 0.004_875).abs() < 1e-12);
    assert_eq!(nearest(&patch, [0.01, 0.001]), [0.01, 0.001]);
}

/// A 16-sided pole of 50 mm along x, a face on top, and the hit on that face's middle.
fn pole() -> (World, Hit) {
    let points: Vec<DVec3> = (0..32)
        .map(|k| {
            let a = f64::from(k % 16) * std::f64::consts::TAU / 16.0 + std::f64::consts::PI / 16.0;
            let (s, c) = det::sin_cos(a);
            DVec3::new(if k < 16 { -0.8 } else { 0.8 }, 0.025 * c, 0.025 * s)
        })
        .collect();
    let hull = Hull::new(&points).expect("a prism").0;
    let top = hull.vertices().iter().map(|v| v.y).fold(0.0, f64::max);
    let mut world = World::new(1.0 / 60.0);
    let b = world.add(Body::rigid(
        Shape::Hull(Arc::new(hull)),
        2.5,
        DVec3::ZERO,
        1.0 / 60.0,
    ));
    let hit = Hit {
        distance: 1.0,
        point: DVec3::new(0.0, top, 0.0),
        normal: DVec3::Y,
        body: Some((b, 0)),
    };
    (world, hit)
}

#[test]
fn a_pole_under_the_feet_carries_its_faces_width_of_the_balance() {
    // 1 kN on a face 9.75 mm across: about the pole's axis the pressure's centre reaches its
    // edge, 4.9 mm (4.9 N·m); along it, half a foot (0.133 m: 133 N·m).
    let (world, hit) = pole();
    let half = 0.025 * det::sin(std::f64::consts::PI / 16.0);
    let along = carried(&world, &hit, (1000.0, 0.6), DVec3::new(10.0, 0.0, 0.0));
    assert!((along.x - 1000.0 * half).abs() < 1e-6, "{along}");
    let across = carried(&world, &hit, (1000.0, 0.6), DVec3::new(0.0, 0.0, 50.0));
    assert!(
        (across - DVec3::new(0.0, 0.0, 50.0)).length() < 1e-9,
        "{across}"
    );
    let beyond = carried(&world, &hit, (1000.0, 0.6), DVec3::new(0.0, 0.0, 400.0));
    assert!(
        (beyond.z - 1000.0 * 0.5 * 0.152 * 1.75).abs() < 1e-6,
        "{beyond}"
    );
    // About the vertical: the soles' friction over the strip they press, μ·N times its mean
    // distance, a little over a quarter of its length.
    let twist = carried(&world, &hit, (1000.0, 0.6), DVec3::new(0.0, 100.0, 0.0));
    assert!(
        twist.y > 0.6 * 1000.0 * 0.25 * 0.266 && twist.y < 0.6 * 1000.0 * 0.07,
        "{twist}"
    );
    // Fixed ground: the balance as it was.
    let ground = Hit { body: None, ..hit };
    let whole = DVec3::new(10.0, 5.0, 50.0);
    assert_eq!(carried(&world, &ground, (1000.0, 0.6), whole), whole);
}

#[test]
fn a_crate_under_the_feet_holds_them_as_the_ground_does() {
    let mut world = World::new(1.0 / 60.0);
    let half = DVec3::new(0.4, 0.25, 0.3);
    let b = world.add(Body::rigid(
        Shape::Box { half },
        30.0,
        DVec3::ZERO,
        1.0 / 60.0,
    ));
    let hit = Hit {
        distance: 1.0,
        point: DVec3::new(0.05, 0.25, 0.0),
        normal: DVec3::Y,
        body: Some((b, 0)),
    };
    let whole = DVec3::new(300.0, 20.0, 500.0);
    assert_eq!(carried(&world, &hit, (1000.0, 0.6), whole), whole);
    // Near its edge the feet hang over: the pressure's centre stays on it, 5 cm out that way,
    // and within the feet's reach the other.
    let edge = Hit {
        point: DVec3::new(0.35, 0.25, 0.0),
        ..hit
    };
    let out = carried(&world, &edge, (1000.0, 0.6), DVec3::new(0.0, 0.0, 300.0));
    assert!((out.z - 1000.0 * 0.05).abs() < 1e-6, "{out}");
    let back = carried(&world, &edge, (1000.0, 0.6), DVec3::new(0.0, 0.0, -300.0));
    assert!((back.z + 1000.0 * 0.133).abs() < 1e-6, "{back}");
}

#[test]
fn a_ball_under_the_feet_carries_no_balance() {
    let mut world = World::new(1.0 / 60.0);
    let b = world.add(Body::rigid(
        Shape::Sphere { radius: 0.2 },
        10.0,
        DVec3::ZERO,
        1.0 / 60.0,
    ));
    let hit = Hit {
        distance: 1.0,
        point: DVec3::new(0.0, 0.2, 0.0),
        normal: DVec3::Y,
        body: Some((b, 0)),
    };
    assert_eq!(
        carried(&world, &hit, (1000.0, 0.6), DVec3::new(10.0, 5.0, 50.0)),
        DVec3::ZERO
    );
}
