//! Items in the world (G1.6a, ADR-0028): what a player puts away is carried on the body to the
//! gram, what does not fit is refused and stays, what is taken out is in the hands, containers
//! hold what fits and weigh it, nothing is lost or made by any of it, and a save keeps it all.

use super::tests::world;
use super::*;
use creeer_net::message::Target;

/// The kit's items by index (`data/kit.ron`).
const COBBLE: u8 = 0;
const PEBBLE: u8 = 1;
const CRATE: u8 = 3;

/// A world with ada standing, and the way east of her.
fn ada() -> (ServerWorld, EntityId, DVec3) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let up = character::plumb(&w.world, at(&w, ada));
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    (w, ada, east)
}

fn wait(w: &mut ServerWorld, ada: EntityId, n: u32) {
    for _ in 0..n {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

fn at(w: &ServerWorld, id: EntityId) -> DVec3 {
    w.world.bodies[w.index(id).unwrap()].structure.position[0]
}

fn mass(w: &ServerWorld, id: EntityId) -> f64 {
    w.world.bodies[w.index(id).unwrap()].structure.masses()[0]
}

/// Drops kit item `item` on the ground `east` metres east of ada, lets it settle; its id.
fn drop(w: &mut ServerWorld, ada: EntityId, east: DVec3, item: u8, metres: f64) -> EntityId {
    let up = character::plumb(&w.world, at(w, ada));
    let where_ = w.ground_under(at(w, ada) + east * metres) + up * 0.3;
    w.apply(&Command::Kit { item, at: where_ }, None).unwrap();
    let id = *w.ids().last().unwrap();
    wait(w, ada, 60);
    id
}

/// The top of `id` as seen now (on its surface, as a player aims).
fn aim(w: &ServerWorld, id: EntityId) -> Target {
    let body = &w.world.bodies[w.index(id).unwrap()];
    let centre = at(w, id);
    let up = character::plumb(&w.world, centre);
    let half = body.shapes[0].1.extent(body.structure.orientation[0], up);
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    Target {
        body: Some(id),
        at: centre + up * half,
        seen,
    }
}

/// Every mass there is: the bodies, and the pockets (whose mass is the avatar's too, so the
/// avatar counts as its dry mass and gas alone).
fn all_mass(w: &ServerWorld, ada: EntityId) -> f64 {
    let bodies: f64 = (0..w.world.bodies.len())
        .map(|i| w.world.bodies[i].structure.masses()[0])
        .sum();
    bodies - w.load(ada) + w.items.load(ada)
}

#[test]
fn a_pebble_put_away_is_carried_on_the_body() {
    let (mut w, ada, east) = ada();
    let pebble = drop(&mut w, ada, east, PEBBLE, 0.8);
    let (before, total) = (mass(&w, ada), all_mass(&w, ada));
    let weight = mass(&w, pebble);
    w.apply(&Command::Stow(Some(aim(&w, pebble))), Some(ada))
        .unwrap();
    assert!(w.index(pebble).is_none(), "the pebble's body is gone");
    assert!((mass(&w, ada) - before - weight).abs() < 1e-12);
    assert!((w.load(ada) - weight).abs() < 1e-15);
    assert!((all_mass(&w, ada) - total).abs() < 1e-9);
    let inventory = w.inventory(ada);
    assert_eq!(inventory.pockets[0].items.len(), 1, "in the first pocket");
    assert!(
        inventory.pockets[0].items[0]
            .text
            .starts_with("granite, 3 cm across")
    );
    assert!(w.take_inventories().contains(&ada), "the player is told");
    // Its gas is the jetpack's still, not the pebble.
    assert!((w.gas(ada) - character::TANK).abs() < 1e-9);
}

#[test]
fn what_fits_no_pocket_is_refused_and_stays() {
    let (mut w, ada, east) = ada();
    let cobble = drop(&mut w, ada, east, COBBLE, 0.8);
    let why = w.apply(&Command::Stow(Some(aim(&w, cobble))), Some(ada));
    assert!(why.unwrap_err().contains("fits no pocket"));
    assert!(w.index(cobble).is_some());
    assert!(w.load(ada) == 0.0);
}

#[test]
fn what_is_taken_out_is_in_the_hands_and_off_the_body() {
    let (mut w, ada, east) = ada();
    let pebble = drop(&mut w, ada, east, PEBBLE, 0.8);
    let before = mass(&w, ada);
    w.apply(&Command::Stow(Some(aim(&w, pebble))), Some(ada))
        .unwrap();
    let total = all_mass(&w, ada);
    w.apply(&Command::TakeOut { pocket: 0, item: 0 }, Some(ada))
        .unwrap();
    let held = w.carried(ada).expect("in the hands");
    assert!((mass(&w, ada) - before).abs() < 1e-12);
    assert!((all_mass(&w, ada) - total).abs() < 1e-9);
    wait(&mut w, ada, 60);
    assert_eq!(w.carried(ada), Some(held), "still held");
    // Put away again from the hands.
    w.apply(&Command::Stow(None), Some(ada)).unwrap();
    assert!(w.carried(ada).is_none() && w.index(held).is_none());
    // Nothing more to take: refused.
    assert!(
        w.apply(&Command::TakeOut { pocket: 0, item: 1 }, Some(ada))
            .is_err()
    );
    assert!(
        w.apply(&Command::TakeOut { pocket: 9, item: 0 }, Some(ada))
            .is_err()
    );
}

#[test]
fn a_crate_holds_what_is_put_in_and_weighs_it() {
    let (mut w, ada, east) = ada();
    let crate_ = drop(&mut w, ada, east, CRATE, 1.0);
    let cobble = drop(&mut w, ada, -east, COBBLE, 0.8);
    let (empty, weight) = (mass(&w, crate_), mass(&w, cobble));
    let total = all_mass(&w, ada);
    w.apply(&Command::Grab(aim(&w, cobble)), Some(ada)).unwrap();
    wait(&mut w, ada, 30);
    w.apply(&Command::PutIn(aim(&w, crate_)), Some(ada))
        .unwrap();
    assert!(w.index(cobble).is_none() && w.carried(ada).is_none());
    assert!((mass(&w, crate_) - empty - weight).abs() < 1e-12);
    assert!((all_mass(&w, ada) - total).abs() < 1e-9);
    let open = w.inventory(ada).open.expect("looked into");
    assert_eq!((open.0, open.1.items.len()), (crate_, 1));
    w.apply(
        &Command::TakeFrom {
            container: aim(&w, crate_),
            item: 0,
        },
        Some(ada),
    )
    .unwrap();
    assert!(w.carried(ada).is_some());
    assert!((mass(&w, crate_) - empty).abs() < 1e-12);
    assert!((all_mass(&w, ada) - total).abs() < 1e-9);
}

#[test]
fn items_out_of_reach_are_refused() {
    let (mut w, ada, east) = ada();
    let pebble = drop(&mut w, ada, east, PEBBLE, 6.0);
    assert!(
        w.apply(&Command::Stow(Some(aim(&w, pebble))), Some(ada))
            .is_err()
    );
    assert!(w.index(pebble).is_some());
}

#[test]
fn a_save_keeps_items_and_pockets() {
    let (mut w, ada, east) = ada();
    let pebble = drop(&mut w, ada, east, PEBBLE, 0.8);
    let crate_ = drop(&mut w, ada, -east, CRATE, 1.0);
    w.apply(&Command::Stow(Some(aim(&w, pebble))), Some(ada))
        .unwrap();
    let bytes = w.to_save().to_bytes();
    let save = creeer_sim::save::Save::from_bytes(&bytes).unwrap();
    let back = ServerWorld::from_save(Arc::clone(&w.data), save, Arc::clone(&w.terrain));
    assert_eq!(back.inventory(ada), w.inventory(ada));
    assert_eq!(back.items.bodies.get(&crate_), w.items.bodies.get(&crate_));
    assert!((back.load(ada) - w.load(ada)).abs() < 1e-15);
    assert!((mass(&back, ada) - mass(&w, ada)).abs() < 1e-15);
}
