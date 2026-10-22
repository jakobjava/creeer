//! Picking up what lies loose (G1.6c, ADR-0029): a stone within reach goes into a pocket or the
//! hands as an item of its rock, once only; one out of reach is refused; what was taken is kept
//! by a save and told to clients.

use super::tests::world;
use super::*;
use creeer_sim::scatter::{Loose, Lying};

/// A world with ada standing at the spawn.
fn ada() -> (ServerWorld, EntityId) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    for _ in 0..60 {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
    (w, ada)
}

fn centre(w: &ServerWorld, id: EntityId) -> DVec3 {
    w.world.bodies[w.index(id).unwrap()].structure.position[0]
}

/// The loose things about ada within `radius` (m), nearest first, those `keep` lets through.
fn about(w: &ServerWorld, ada: EntityId, radius: f64, keep: impl Fn(&Loose) -> bool) -> Vec<Loose> {
    let at = centre(w, ada);
    let scatter = w.items.loose.as_ref().unwrap();
    let mut all: Vec<Loose> = (scatter.cells_near(at, radius).into_iter())
        .flat_map(|c| scatter.in_cell(&w.terrain, c))
        .filter(|l| (l.centre - at).length() < radius && keep(l))
        .collect();
    all.sort_by(|a, b| {
        (a.centre - at)
            .length()
            .total_cmp(&(b.centre - at).length())
    });
    all
}

/// Ada walks to stand by `to` (within a metre of it).
fn walk_to(w: &mut ServerWorld, ada: EntityId, to: DVec3) {
    for _ in 0..600 {
        let at = centre(w, ada);
        let up = character::plumb(&w.world, at);
        let way = (to - at).reject_from(up);
        if way.length() < 0.8 {
            break;
        }
        let walk = AvatarInput {
            walk: way.normalize() * 1.2,
            ..AvatarInput::default()
        };
        w.steer(ada, &walk);
        w.step();
    }
    for _ in 0..30 {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

fn small_stone(l: &Loose) -> bool {
    matches!(l.what, Lying::Stone { size, .. } if size.x < 0.05)
}

#[test]
fn a_stone_in_reach_goes_into_a_pocket_once() {
    let (mut w, ada) = ada();
    let stone = about(&w, ada, 40.0, small_stone)[0];
    walk_to(&mut w, ada, stone.centre);
    let before = w.load(ada);
    w.apply(
        &Command::Pick {
            loose: stone.id,
            pocket: true,
        },
        Some(ada),
    )
    .unwrap();
    let item = w.items.loose.as_ref().unwrap().item(
        &stone,
        &w.items.rocks,
        w.items.densities.as_ref().unwrap(),
    );
    assert!((w.load(ada) - before - item.unwrap().mass()).abs() < 1e-12);
    assert_eq!(w.taken_since(0), [stone.id]);
    let again = w.apply(
        &Command::Pick {
            loose: stone.id,
            pocket: true,
        },
        Some(ada),
    );
    assert!(again.unwrap_err().contains("taken"));
}

#[test]
fn a_stone_picked_into_the_hands_is_held_as_an_item_of_its_colour() {
    let (mut w, ada) = ada();
    let stone = about(&w, ada, 40.0, |l| matches!(l.what, Lying::Stone { .. }))[0];
    walk_to(&mut w, ada, stone.centre);
    w.apply(
        &Command::Pick {
            loose: stone.id,
            pocket: false,
        },
        Some(ada),
    )
    .unwrap();
    let held = w.carried(ada).expect("in the hands");
    let Lying::Stone { rock, .. } = stone.what else {
        unreachable!()
    };
    assert_eq!(
        w.item_colour(held),
        Some(w.items.rocks.rocks[usize::from(rock)].color)
    );
    for _ in 0..60 {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
    assert_eq!(w.carried(ada), Some(held), "still held");
}

#[test]
fn a_stone_out_of_reach_is_refused() {
    let (mut w, ada) = ada();
    let at = centre(&w, ada);
    let far = about(&w, ada, 60.0, |l| (l.centre - at).length() > 10.0)[0];
    let why = w
        .apply(
            &Command::Pick {
                loose: far.id,
                pocket: true,
            },
            Some(ada),
        )
        .unwrap_err();
    assert!(why.contains("out of reach"), "{why}");
    assert!(w.taken_since(0).is_empty());
}

#[test]
fn a_save_keeps_what_was_taken() {
    let (mut w, ada) = ada();
    let stone = about(&w, ada, 40.0, small_stone)[0];
    walk_to(&mut w, ada, stone.centre);
    w.apply(
        &Command::Pick {
            loose: stone.id,
            pocket: true,
        },
        Some(ada),
    )
    .unwrap();
    let save = creeer_sim::save::Save::from_bytes(&w.to_save().to_bytes()).unwrap();
    let mut back = ServerWorld::from_save(Arc::clone(&w.data), save, Arc::clone(&w.terrain));
    assert_eq!(back.taken_since(0), [stone.id]);
    let again = back.apply(
        &Command::Pick {
            loose: stone.id,
            pocket: true,
        },
        Some(ada),
    );
    assert!(again.is_err());
}
