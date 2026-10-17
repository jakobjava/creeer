//! Islands of bodies, which of them may be parked, and the parked islands found by place.

use crate::lod::{ACTIVE, Island, MARGIN, PARK, Parked, bounds, islands, to_park};
use creeer_math::glam::DVec3;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};

const DT: f64 = 1.0 / 60.0;

fn crate_at(p: DVec3) -> Body {
    Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        200.0,
        p,
        DT,
    )
}

#[test]
fn a_bodys_ball_holds_its_shapes() {
    let (c, r) = bounds(&crate_at(DVec3::new(1.0, 2.0, 3.0)));
    assert!((c - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-12);
    // The box's own ball (half-diagonal 0.866) inside the box of balls about it (1.5).
    assert!(r >= 0.75f64.sqrt() && r <= 1.5, "{r}");
}

#[test]
fn touching_bodies_are_one_island_and_apart_ones_are_not() {
    let mut world = World::new(DT);
    // A stack of three (each ball reaches the next), a crate alone, a row of ten in a line.
    for k in 0..3 {
        world.add(crate_at(DVec3::new(0.0, f64::from(k), 0.0)));
    }
    world.add(crate_at(DVec3::new(50.0, 0.0, 0.0)));
    for k in 0..10 {
        world.add(crate_at(DVec3::new(100.0 + 1.5 * f64::from(k), 0.0, 0.0)));
    }
    let found = islands(&world);
    let members: Vec<Vec<usize>> = found.iter().map(|(m, _)| m.clone()).collect();
    assert_eq!(
        members,
        [vec![0, 1, 2], vec![3], (4..14).collect::<Vec<_>>()]
    );
    // The row's ball holds every crate of it.
    let (centre, radius) = found[2].1;
    for i in 4..14 {
        let (c, r) = bounds(&world.bodies[i]);
        assert!((c - centre).length() + r <= radius + 1e-9);
    }
    // Two crates further apart than their balls and the margin are two islands.
    let mut apart = World::new(DT);
    let (_, r) = bounds(&crate_at(DVec3::ZERO));
    apart.add(crate_at(DVec3::ZERO));
    apart.add(crate_at(DVec3::X * (2.0 * r + MARGIN * 1.01)));
    assert_eq!(islands(&apart).len(), 2);
}

#[test]
fn only_resting_islands_far_from_every_player_are_parked() {
    let mut world = World::new(DT);
    let far = DVec3::new(PARK + 100.0, 0.0, 0.0);
    let near = DVec3::new(PARK - 100.0, 0.0, 0.0);
    for p in [far, far + DVec3::Y, near] {
        world.add(crate_at(p));
    }
    world.add(crate_at(far + DVec3::Z * 50.0));
    for (i, body) in world.bodies.iter_mut().enumerate() {
        body.sleep.asleep = i != 3;
    }
    // The stack far away (asleep) goes; the crate nearer than PARK stays; the far one awake
    // stays (it moves: simulated until it rests).
    assert_eq!(to_park(&world, &[DVec3::ZERO]), [vec![0, 1]]);
    // With no player at all every resting island goes.
    assert_eq!(to_park(&world, &[]), [vec![0, 1], vec![2]]);
    // A second player near the stack keeps it.
    assert!(to_park(&world, &[DVec3::ZERO, far]).is_empty());
}

#[test]
fn parked_islands_are_found_near_players_oldest_first() {
    let mut parked: Parked<u32> = Parked::default();
    let place = |x: f64| Island::new(vec![(crate_at(DVec3::new(x, 0.0, 0.0)), 7)]);
    let a = parked.park(place(10_000.0));
    let b = parked.park(place(-10_000.0));
    let c = parked.park(place(10_000.0 + ACTIVE - 2.0));
    assert_eq!((a, b, c), (0, 1, 2));
    // From 10 km: a (here) and c (just within reach, its ball nearer still).
    assert_eq!(
        parked.near(&[DVec3::new(10_000.0, 0.0, 0.0)], ACTIVE),
        [0, 2]
    );
    assert_eq!(
        parked.near(
            &[
                DVec3::new(10_000.0, 0.0, 0.0),
                DVec3::new(-10_000.0, 0.0, 0.0)
            ],
            ACTIVE
        ),
        [0, 1, 2]
    );
    assert!(parked.near(&[DVec3::ZERO], ACTIVE).is_empty());
    let island = parked.take(a).unwrap();
    assert_eq!(island.bodies[0].1, 7);
    assert_eq!(parked.near(&[DVec3::new(10_000.0, 0.0, 0.0)], ACTIVE), [2]);
    assert!(parked.take(a).is_none());
    assert_eq!(parked.counts(), (2, 2));
    assert_eq!(parked.find(|&t| t == 7), Some(1));
}
