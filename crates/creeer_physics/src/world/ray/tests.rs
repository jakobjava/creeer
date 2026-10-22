//! Rays meet what they first reach, where analytic geometry says.

use super::*;
use crate::world::Body;
use crate::world::hull::Hull;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;

/// A ball planet of radius `r` at the origin (its density the distance to its surface).
#[derive(Debug)]
struct Ball(f64);

impl crate::world::ground::Ground for Ball {
    fn density_gradient(&self, p: DVec3) -> (f64, DVec3) {
        (p.length() - self.0, p.normalize_or_zero())
    }
}

fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < 1e-9, "{what}: {a} against {b}");
}

#[test]
fn a_ray_meets_the_first_of_planes_boxes_balls_and_hulls() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let turn = DQuat::from_rotation_z(0.3);
    world.statics.push(StaticShape::Box {
        center: DVec3::new(5.0, 0.0, 0.0),
        rotation: turn,
        half: DVec3::splat(0.5),
    });
    let ball = world.add(Body::rigid(
        Shape::Sphere { radius: 0.5 },
        1.0,
        DVec3::new(0.0, 2.0, 0.0),
        DT,
    ));
    let hull = Arc::new(Hull::cuboid(DVec3::new(0.3, 0.2, 0.4)));
    let block = world.add(Body::rigid(
        Shape::Hull(hull),
        1.0,
        DVec3::new(-3.0, 2.0, 0.0),
        DT,
    ));
    // Down onto the plane, from above.
    let hit = world
        .ray(DVec3::new(2.0, 3.0, 0.0), DVec3::NEG_Y, 10.0, None)
        .unwrap();
    close(hit.distance, 3.0, "plane");
    assert_eq!((hit.normal, hit.body), (DVec3::Y, None));
    // Down onto the ball: it is nearer than the plane under it.
    let hit = world
        .ray(DVec3::new(0.0, 5.0, 0.0), DVec3::NEG_Y, 10.0, None)
        .unwrap();
    close(hit.distance, 2.5, "ball");
    assert_eq!(hit.body, Some((ball, 0)));
    // Its own body skipped: the plane under it.
    let hit = world
        .ray(DVec3::new(0.0, 5.0, 0.0), DVec3::NEG_Y, 10.0, Some(ball))
        .unwrap();
    close(hit.distance, 5.0, "skipped");
    // Sideways into the turned box: its face along the ray's way, turned 0.3 rad.
    let hit = world
        .ray(DVec3::new(2.0, 0.1, 0.0), DVec3::X, 10.0, None)
        .unwrap();
    let face = turn * DVec3::NEG_X;
    let want = (DVec3::new(5.0, 0.0, 0.0) + face * 0.5 - DVec3::new(2.0, 0.1, 0.0)).dot(face)
        / DVec3::X.dot(face);
    close(hit.distance, want, "turned box");
    close(hit.normal.dot(face), 1.0, "its normal");
    // Into the hull, as into a box of its size.
    let hit = world
        .ray(DVec3::new(-3.0, 2.0, 5.0), DVec3::NEG_Z, 10.0, None)
        .unwrap();
    close(hit.distance, 4.6, "hull");
    assert_eq!(hit.body, Some((block, 0)));
    // Nothing within reach.
    assert!(
        world
            .ray(DVec3::new(10.0, 3.0, 0.0), DVec3::NEG_Y, 2.0, None)
            .is_none()
    );
    assert!(
        world
            .ray(DVec3::new(10.0, 3.0, 0.0), DVec3::Y, 50.0, None)
            .is_none()
    );
}

#[test]
fn a_ray_meets_the_ground_where_its_surface_is() {
    let mut world = World::new(DT);
    world.ground = Some(Arc::new(Ball(1000.0)));
    // Straight down from 1.5 m above it, and slanting.
    let o = DVec3::new(0.0, 1001.5, 0.0);
    let hit = world.ray(o, DVec3::NEG_Y, 3.0, None).unwrap();
    assert!((hit.distance - 1.5).abs() < 2e-5, "{}", hit.distance);
    assert!((hit.normal - DVec3::Y).length() < 1e-6);
    let d = DVec3::new(0.6, -0.8, 0.0);
    let hit = world.ray(o, d, 5.0, None).unwrap();
    // |o + t·d| = R: t² + 2t·(o·d) + |o|² − R² = 0.
    let (b, c) = (o.dot(d), o.length_squared() - 1000.0 * 1000.0);
    let want = -b - (b * b - c).sqrt();
    assert!(
        (hit.distance - want).abs() < 2e-5,
        "{} against {want}",
        hit.distance
    );
    // Too short, or pointing away.
    assert!(world.ray(o, DVec3::NEG_Y, 1.0, None).is_none());
    assert!(world.ray(o, DVec3::Y, 100.0, None).is_none());
    // Starting inside it: at once.
    assert_eq!(
        world
            .ray(DVec3::new(0.0, 999.0, 0.0), DVec3::Y, 5.0, None)
            .unwrap()
            .distance,
        0.0
    );
}

/// Counts its density evaluations.
#[derive(Debug)]
struct Counted(Ball, std::sync::atomic::AtomicUsize);

impl crate::world::ground::Ground for Counted {
    fn density_gradient(&self, p: DVec3) -> (f64, DVec3) {
        self.1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.0.density_gradient(p)
    }
}

#[test]
fn a_ray_finds_the_ground_in_a_few_evaluations() {
    let ground = Arc::new(Counted(
        Ball(1000.0),
        std::sync::atomic::AtomicUsize::new(0),
    ));
    let mut world = World::new(DT);
    world.ground = Some(ground.clone());
    // A leg's ray: 0.92 m down to the ground, 1.1 m long.
    let hit = world
        .ray(DVec3::new(0.0, 1000.92, 0.0), DVec3::NEG_Y, 1.1, None)
        .unwrap();
    assert!((hit.distance - 0.92).abs() < 1e-5);
    let n = ground.1.load(std::sync::atomic::Ordering::Relaxed);
    assert!(n <= 4, "{n} evaluations");
}

#[test]
fn a_point_lies_as_far_outside_a_shape_as_geometry_says() {
    let turn = DQuat::from_rotation_y(0.7);
    let c = DVec3::new(1.0, 2.0, 3.0);
    let at = |local: DVec3| c + turn * local;
    let ball = Shape::Sphere { radius: 0.5 };
    close(
        ball.distance(c, turn, at(DVec3::X * 0.8)),
        0.3,
        "outside a ball",
    );
    close(
        ball.distance(c, turn, at(DVec3::Y * 0.1)),
        -0.4,
        "inside a ball",
    );
    let half = DVec3::new(0.3, 0.2, 0.4);
    let block = Shape::Box { half };
    close(
        block.distance(c, turn, at(DVec3::new(0.5, 0.0, 0.0))),
        0.2,
        "off a face",
    );
    close(
        block.distance(c, turn, at(DVec3::new(0.6, 0.6, 0.4))),
        0.5,
        "off an edge",
    );
    close(
        block.distance(c, turn, at(DVec3::new(0.0, 0.15, 0.0))),
        -0.05,
        "inside a box",
    );
    // A hull of the box's size: the same off a face and inside, nearer off an edge.
    let hull = Shape::Hull(Arc::new(Hull::cuboid(half)));
    close(
        hull.distance(c, turn, at(DVec3::new(0.5, 0.0, 0.0))),
        0.2,
        "off a hull's face",
    );
    close(
        hull.distance(c, turn, at(DVec3::new(0.0, 0.15, 0.0))),
        -0.05,
        "inside a hull",
    );
    let edge = hull.distance(c, turn, at(DVec3::new(0.6, 0.6, 0.4)));
    assert!(
        (0.4 - 1e-9..=0.5).contains(&edge),
        "off a hull's edge: {edge}"
    );
    // A ray into each, from its own frame's +x: the face at its half width.
    for shape in [&ball, &block, &hull] {
        let (t, n) = shape
            .ray(c, turn, at(DVec3::X * 2.0), turn * DVec3::NEG_X)
            .unwrap();
        let face = if let Shape::Sphere { radius } = shape {
            *radius
        } else {
            half.x
        };
        close(t, 2.0 - face, "a shape's ray");
        close(n.dot(turn * DVec3::X), 1.0, "its normal");
    }
}

#[test]
fn a_ground_ray_passes_the_bodies_by() {
    let mut world = World::new(DT);
    world.ground = Some(Arc::new(Ball(1000.0)));
    world.add(Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        1.0,
        DVec3::new(0.0, 1001.0, 0.0),
        DT,
    ));
    let o = DVec3::new(0.0, 1003.0, 0.0);
    // The box first for the whole world's ray, the ground for the ground's.
    let all = world.ray(o, DVec3::NEG_Y, 5.0, None).unwrap();
    close(all.distance, 1.5, "the box");
    let ground = world.ground_ray(o, DVec3::NEG_Y, 5.0).unwrap();
    assert!((ground.distance - 3.0).abs() < 2e-5, "{}", ground.distance);
    assert_eq!(ground.body, None);
    assert!(world.ground_ray(o, DVec3::NEG_Y, 2.5).is_none());
}

#[test]
fn a_shape_reaches_as_far_as_its_farthest_point() {
    let turn = DQuat::from_rotation_z(0.4);
    let half = DVec3::new(0.3, 0.2, 0.4);
    let d = DVec3::new(0.6, 0.8, 0.0);
    let local = turn.conjugate() * d;
    let want = (local.x * 0.3).abs() + (local.y * 0.2).abs();
    close(Shape::Box { half }.extent(turn, d), want, "a box");
    close(
        Shape::Hull(Arc::new(Hull::cuboid(half))).extent(turn, d),
        want,
        "a hull",
    );
    close(Shape::Sphere { radius: 0.5 }.extent(turn, d), 0.5, "a ball");
}
