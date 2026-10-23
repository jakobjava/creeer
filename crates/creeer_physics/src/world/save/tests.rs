//! A saved world loads as it was (saving it again gives the same bytes), goes on as the
//! original does, and bytes that are not a world are refused, never crash.

use super::*;
use crate::structure::Section;
use crate::structure::section::STEEL_S235;
use crate::structure::shapes::{Base, space_frame};
use crate::world::hull::Hull;
use creeer_math::glam::{DQuat, DVec3};

const DT: f64 = 1.0 / 60.0;

/// Crates stacked, a ball, a rock and a steel frame that hits a wall and bends for good.
fn scene() -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world.statics.push(StaticShape::Box {
        center: DVec3::new(12.0, 1.0, 1.0),
        rotation: DQuat::IDENTITY,
        half: DVec3::new(0.5, 2.0, 3.0),
    });
    for k in 0..3 {
        let at = DVec3::new(-4.0, 0.5 + 1.0 * f64::from(k), 0.0);
        let mut b = Body::rigid(
            Shape::Box {
                half: DVec3::splat(0.5),
            },
            50.0,
            at,
            DT,
        );
        b.friction = 0.7;
        world.add(b);
    }
    let mut ball = Body::rigid(
        Shape::Sphere { radius: 0.3 },
        20.0,
        DVec3::new(0.0, 2.0, 3.0),
        DT,
    );
    ball.structure.velocity[0] = DVec3::new(1.0, 0.0, -0.5);
    world.add(ball);
    let points = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.2, 0.1, 0.0),
        DVec3::new(0.2, 0.9, 0.1),
        DVec3::new(0.1, 0.2, 1.1),
        DVec3::new(0.8, 0.7, 0.9),
    ];
    let (hull, centre) = Hull::new(&points).unwrap();
    let rock = Body::rigid(
        Shape::Hull(Arc::new(hull)),
        300.0,
        centre + DVec3::new(4.0, 1.0, -3.0),
        DT,
    );
    world.add(rock);
    let frame = space_frame(
        [2, 1, 1],
        2.0,
        Section::round_tube(0.08, 0.004),
        STEEL_S235,
        Base::Free,
    );
    let mut body = Body::structure(frame, DT);
    for p in &mut body.structure.position {
        p.y += 0.05;
    }
    for v in &mut body.structure.velocity {
        *v = DVec3::X * 12.0;
    }
    world.add(body);
    world
}

fn positions(world: &World) -> Vec<DVec3> {
    world
        .bodies
        .iter()
        .flat_map(|b| b.structure.position.iter().copied())
        .collect()
}

#[test]
fn a_saved_world_loads_as_it_was_and_goes_on_as_it_would_have() {
    let mut world = scene();
    // (The frame meets the wall at tick 45 or 46, by how its slide went; G1.8.)
    for _ in 0..50 {
        world.step();
    }
    let frame = world.bodies.last().unwrap();
    let bent = frame
        .structure
        .elements
        .iter()
        .any(|e| e.plastic_strain > 0.0);
    assert!(bent, "the frame should have hit the wall and bent");
    let bytes = world.to_bytes();
    let mut loaded = World::from_bytes(&bytes).expect("a world");
    assert_eq!(
        loaded.to_bytes(),
        bytes,
        "saving the loaded world changes it"
    );
    assert_eq!(positions(&loaded), positions(&world));
    // Two seconds on, the frame bending further: just where the original goes, bit for bit
    // (its solver builds the very matrix the original had).
    for tick in 0..120 {
        world.step();
        loaded.step();
        assert_eq!(positions(&loaded), positions(&world), "tick {tick}");
    }
    let asleep = |w: &World| w.bodies.iter().filter(|b| b.sleep.asleep).count();
    assert_eq!(asleep(&loaded), asleep(&world));
}

#[test]
fn bytes_that_are_not_a_world_are_refused() {
    let mut world = scene();
    for _ in 0..10 {
        world.step();
    }
    let bytes = world.to_bytes();
    // Cut short anywhere: refused.
    for cut in (0..bytes.len()).step_by(97) {
        assert!(World::from_bytes(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    // Bytes changed here and there: refused or read, never a crash (and what is read saves
    // as read).
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..300 {
        let mut changed = bytes.clone();
        for _ in 0..3 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            #[allow(clippy::cast_possible_truncation)]
            let at = (state % bytes.len() as u64) as usize;
            changed[at] ^= state.to_le_bytes()[4] | 1;
        }
        if let Ok(read) = World::from_bytes(&changed) {
            assert_eq!(
                World::from_bytes(&read.to_bytes()).unwrap().to_bytes(),
                read.to_bytes()
            );
        }
    }
}
