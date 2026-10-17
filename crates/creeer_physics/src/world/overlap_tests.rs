//! Fast-turning bodies never sink into each other: the deepest overlap of any two bodies (or a
//! body and fixed geometry) at the end of any step, while crates are smashed and rocks tumble
//! into a heap. It holds the turn limit (`turn.rs`) to what it is for: at 0.05 rad a substep
//! the deepest was 4.1 mm (smash) and 6.1 mm (heap); 0.1 rad gave 13 mm, 0.2 rad 53 mm
//! (measured 2026-10-18), where contacts that only meet after the turn went unseen.

use super::hull::Hull;
use super::hull_contact_tests::deepest;
use super::hull_tests::cloud;
use super::narrow::{self, pose};
use super::shape::{Shape, StaticShape};
use super::*;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;

/// The deepest overlap now (m, positive into each other).
fn overlap(world: &World) -> f64 {
    let mut gap = f64::INFINITY;
    for (i, a) in world.bodies.iter().enumerate() {
        let pa = pose(a.structure.position[0], a.structure.orientation[0]);
        let shape = &a.shapes[0].1;
        for s in &world.statics {
            gap = gap.min(deepest(&narrow::with_static((pa, shape), s, 0.0)));
        }
        for b in &world.bodies[i + 1..] {
            let pb = pose(b.structure.position[0], b.structure.orientation[0]);
            gap = gap.min(deepest(&narrow::shapes(
                (pa, shape),
                (pb, &b.shapes[0].1),
                0.0,
            )));
        }
    }
    (-gap).max(0.0)
}

/// The deepest overlap over `ticks` steps (m) and the tick it was at.
fn deepest_over(mut world: World, ticks: usize) -> (f64, usize) {
    let (mut most, mut at) = (0.0f64, 0);
    for k in 0..ticks {
        world.step();
        let o = overlap(&world);
        if o > most {
            (most, at) = (o, k);
        }
    }
    (most, at)
}

fn smash(speed: f64) -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    for stack in 0..5 {
        for level in 0..6 {
            let at = DVec3::new(f64::from(stack) * 1.4 - 2.8, 0.5 + f64::from(level), 0.0);
            let mut c = Body::rigid(
                Shape::Box {
                    half: DVec3::splat(0.5),
                },
                80.0,
                at,
                DT,
            );
            c.friction = 0.6;
            world.add(c);
        }
    }
    let mut ball = Body::rigid(
        Shape::Sphere { radius: 0.6 },
        800.0,
        DVec3::new(-2.8, 3.5, -12.0),
        DT,
    );
    ball.structure.velocity[0] = DVec3::new(0.0, 2.0, speed);
    ball.restitution = 0.3;
    world.add(ball);
    world
}

fn heap(seed: u64) -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    for k in 0..12u32 {
        let flat: Vec<DVec3> = cloud(10, 0.3, u64::from(k) + seed)
            .into_iter()
            .map(|p| p * DVec3::new(1.0, 0.7, 0.45))
            .collect();
        let (hull, _) = Hull::new(&flat).expect("a rock");
        let at = DVec3::new(
            0.3 * f64::from(k % 3) - 0.3,
            0.5 + 0.7 * f64::from(k),
            0.2 * f64::from(k % 2),
        );
        let mass = 2600.0 * hull.volume();
        let mut rock = Body::rigid(Shape::Hull(Arc::new(hull)), mass, at, DT);
        rock.structure.orientation[0] = DQuat::from_axis_angle(
            DVec3::new(1.0, f64::from(k), 0.3).normalize(),
            0.9 * f64::from(k),
        );
        rock.surface = surface::Surface::ROCK;
        rock.friction = 0.7;
        world.add(rock);
    }
    world
}

#[test]
fn fast_turning_bodies_never_sink_into_each_other() {
    let (most, at) = deepest_over(smash(14.5), 120);
    assert!(
        most < 5e-3,
        "a smash: {:.2} mm deep at tick {at}",
        most * 1e3
    );
    let (most, at) = deepest_over(heap(13), 300);
    assert!(
        most < 8e-3,
        "a heap: {:.2} mm deep at tick {at}",
        most * 1e3
    );
}
