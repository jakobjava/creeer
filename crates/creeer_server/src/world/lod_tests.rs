//! Parking and waking in the planet's world: bodies come back bit for bit, moving bodies
//! stay until they rest, parked islands are saved, an offline player's avatar comes back.

use super::tests::{above, world};
use super::*;
use creeer_sim::save::Save;

const FAR: f64 = 4_000.0;

/// The bodies of `world` with ids `ids` (wherever they are in it) as bytes.
fn bytes_of(world: &ServerWorld, ids: &[EntityId]) -> Vec<u8> {
    let mut w = World::new(DT);
    w.bodies = ids
        .iter()
        .map(|&id| world.world.bodies[world.index(id).expect("in the world")].clone())
        .collect();
    w.to_bytes()
}

/// Steps `world` `ticks` times with its LOD pass around `centre` (none: no pass), its
/// avatars asked for nothing (braked to rest, as the server does with absent players').
fn run(world: &mut ServerWorld, ticks: u32, centre: Option<DVec3>) {
    for _ in 0..ticks {
        for id in world.avatars() {
            world.steer(id, &creeer_net::message::AvatarInput::default());
        }
        if let Some(c) = centre {
            world.regions(&[c]);
        }
        world.step();
    }
}

/// A world with Ada at the spawn and a stack of `n` crates `FAR` metres east.
fn stack(n: u32) -> (ServerWorld, EntityId, Vec<EntityId>) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    let ids = (0..n)
        .map(|k| {
            let at = above(&w, FAR, 0.6 + 1.05 * f64::from(k));
            w.apply(&Command::Crate(at), None).unwrap();
            *w.ids().last().unwrap()
        })
        .collect();
    (w, ada, ids)
}

fn position(world: &ServerWorld, id: EntityId) -> DVec3 {
    world.world.bodies[world.index(id).unwrap()]
        .structure
        .position[0]
}

#[test]
fn a_resting_stack_far_away_is_parked_and_comes_back_bit_for_bit() {
    let (mut parking, ada, ids) = stack(3);
    let mut plain = ServerWorld::from_save(Data::base(), parking.to_save(), terrain_of(&parking));
    let centre = position(&parking, ada);
    // Until it rests it is simulated; then it goes, in one island.
    let mut ticks = 0;
    while parking.parked().1 == 0 {
        run(&mut parking, 10, Some(centre));
        ticks += 10;
        assert!(ticks < 3000, "never parked");
    }
    assert_eq!(parking.parked(), (1, 3));
    assert!(
        ids.iter()
            .all(|&id| parking.is_parked(id) && parking.index(id).is_none())
    );
    // The world without the pass, stepped as long and a while more: the same as the
    // parked world's for everything left in it, and the stack, still asleep, as parked.
    run(&mut plain, ticks + 200, None);
    run(&mut parking, 200, Some(centre));
    assert_eq!(bytes_of(&parking, &[ada]), bytes_of(&plain, &[ada]));
    // A player comes near: back, bit for bit as the stack that never left. (Ada, far from
    // them and asleep, is parked in turn.)
    let near = position(&plain, ids[0]);
    while ids.iter().any(|&id| parking.is_parked(id)) {
        parking.regions(&[near]);
        parking.step();
        plain.step();
    }
    assert_eq!(bytes_of(&parking, &ids), bytes_of(&plain, &ids));
    assert!(ids.iter().all(|&id| !parking.is_parked(id)));
}

fn terrain_of(world: &ServerWorld) -> Arc<Terrain> {
    let terrain = super::tests::terrain();
    terrain.set_edits(world.terrain.edits().clone());
    terrain
}

#[test]
fn a_toppling_tower_stays_simulated_until_it_rests() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    let centre = position(&w, ada);
    // Six crates, each set 30 cm further east than the one under it: it falls.
    for k in 0..6 {
        let at = above(&w, FAR + 0.3 * f64::from(k), 0.6 + 1.05 * f64::from(k));
        w.apply(&Command::Crate(at), None).unwrap();
    }
    let top = *w.ids().last().unwrap();
    let start = position(&w, top);
    let mut ticks = 0;
    while w.parked().1 < 6 {
        run(&mut w, 10, Some(centre));
        ticks += 10;
        // Whatever is parked was at rest: nothing is taken out while it moves.
        let resting = (w.parked.iter()).all(|i| i.bodies.iter().all(|(b, _)| b.sleep.asleep));
        assert!(resting, "parked while moving");
        assert!(
            ticks < 6000,
            "the fallen tower never rested: {:?} parked",
            w.parked()
        );
    }
    // It fell (the top crate is far from where it was set) before it was parked.
    let island = w
        .parked
        .iter()
        .find(|i| i.bodies.iter().any(|(_, t)| t.id == top))
        .unwrap();
    let (body, _) = island.bodies.iter().find(|(_, t)| t.id == top).unwrap();
    let fell = (body.structure.position[0] - start).length();
    assert!(fell > 1.5, "it fell {fell} m");
    assert!(ticks > 60, "parked after {ticks} ticks");
}

#[test]
fn parked_islands_are_saved_and_loaded() {
    let (mut w, ada, ids) = stack(2);
    let centre = position(&w, ada);
    while w.parked().1 == 0 {
        run(&mut w, 10, Some(centre));
    }
    let bytes = w.to_save().to_bytes();
    let mut loaded = ServerWorld::from_save(
        Data::base(),
        Save::from_bytes(&bytes).unwrap(),
        terrain_of(&w),
    );
    assert_eq!(loaded.parked(), w.parked());
    assert_eq!(loaded.to_save().to_bytes(), bytes);
    let near = above(&w, FAR, 1.0);
    for world in [&mut w, &mut loaded] {
        while ids.iter().any(|&id| world.is_parked(id)) {
            world.regions(&[near]);
            world.step();
        }
    }
    assert_eq!(bytes_of(&loaded, &ids), bytes_of(&w, &ids));
}

#[test]
fn an_offline_players_avatar_is_parked_and_back_when_they_return() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    // Bob walked far east before he left.
    let east = FAR;
    let feet = above(&w, east, 0.02);
    let body = creeer_sim::character::body(feet, feet.normalize(), DT);
    let bob = w.add_body(body, Some("bob".into()));
    let centre = position(&w, ada);
    let mut ticks = 0;
    while !w.is_parked(bob) {
        run(&mut w, 10, Some(centre));
        ticks += 10;
        assert!(ticks < 3000, "never parked");
    }
    assert_eq!(w.avatars(), [ada]);
    // He comes back: his avatar, where he left it.
    assert_eq!(w.avatar_of("bob"), bob);
    assert!(!w.is_parked(bob));
    assert!((position(&w, bob) - above(&w, east, 0.0)).length() < 2.0);
    assert_eq!(w.player(w.index(bob).unwrap()), Some("bob"));
}
