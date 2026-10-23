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

#[test]
fn a_branch_picked_up_is_carried_steadily_walking() {
    // (Pulled at their grip, off their axis, branches spun up to 33 rad/s as the player walked.)
    let (mut w, ada) = ada();
    let branch = about(&w, ada, 60.0, |l| matches!(l.what, Lying::Branch { .. }))[0];
    walk_to(&mut w, ada, branch.centre);
    w.apply(
        &Command::Pick {
            loose: branch.id,
            pocket: false,
        },
        Some(ada),
    )
    .unwrap();
    let up = character::plumb(&w.world, centre(&w, ada));
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let mut most: f64 = 0.0;
    for t in 0..300 {
        let walk = if t > 60 { east * 1.3 } else { DVec3::ZERO };
        w.steer(
            ada,
            &AvatarInput {
                walk,
                ..AvatarInput::default()
            },
        );
        w.step();
        let i = w.carried(ada).and_then(|c| w.index(c)).expect("still held");
        // Once the body has turned the way it walks.
        if t > 150 {
            most = most.max(w.world.bodies[i].structure.angular_velocity[0].length());
        }
    }
    assert!(most < 1.0, "spins at {most} rad/s");
}

#[test]
fn a_stone_lying_struck_with_a_stone_in_hand_breaks_where_it_lies() {
    let (mut w, ada) = ada();
    // The nearest stone of a few centimetres, struck with a hammerstone.
    let stone = about(
        &w,
        ada,
        40.0,
        |l| matches!(l.what, Lying::Stone { size, .. } if size.x > 0.05),
    )[0];
    walk_to(&mut w, ada, stone.centre);
    let hammer = creeer_sim::items::Item::Piece(
        creeer_sim::items::Piece::solid(
            "basalt",
            2900.0,
            creeer_sim::items::Form::Lump {
                size: DVec3::new(0.10, 0.08, 0.06),
            },
        ),
        Vec::new(),
    );
    w.put_in_hands(ada, hammer).unwrap();
    for _ in 0..60 {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
    let before = w.items.bodies.len();
    #[allow(clippy::cast_precision_loss)]
    let now = w.tick as f64;
    w.apply(
        &Command::StrikeLoose {
            loose: stone.id,
            started: now,
        },
        Some(ada),
    )
    .unwrap();
    assert_eq!(w.taken_since(0), [stone.id], "lying no longer: an item");
    assert_eq!(w.items.bodies.len(), before + 1);
    for _ in 0..70 {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
    // The blow fell on it: what broke off lies beside it (a flake, or grit), or it bruised.
    let Lying::Stone { size, rock } = stone.what else {
        unreachable!()
    };
    let breaks = w.items.rocks.rocks[usize::from(rock)].breaks.is_some();
    assert!(!breaks || w.items.laid.len() >= 2, "{size:?} broke nothing");
    // Once struck it is taken: not struck again as lying (the swing's cycle over first).
    for _ in 0..60 {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
    #[allow(clippy::cast_precision_loss)]
    let later = w.tick as f64;
    let again = w.apply(
        &Command::StrikeLoose {
            loose: stone.id,
            started: later,
        },
        Some(ada),
    );
    assert!(again.unwrap_err().contains("taken"));
}

#[test]
fn knapping_stones_lying_about_costs_the_world_next_to_nothing() {
    // (Flakes as bodies settling on the terrain took 50–190 Newton iterations a tick each, and a
    // few of them froze the game: what breaks off, and what lies still, is laid instead.)
    let (mut w, ada) = ada();
    let first = about(
        &w,
        ada,
        40.0,
        |l| matches!(l.what, Lying::Stone { size, .. } if size.x > 0.05),
    );
    walk_to(&mut w, ada, first[0].centre);
    let hammer = creeer_sim::items::Item::Piece(
        creeer_sim::items::Piece::solid(
            "basalt",
            2900.0,
            creeer_sim::items::Form::Lump {
                size: DVec3::new(0.10, 0.08, 0.06),
            },
        ),
        Vec::new(),
    );
    w.put_in_hands(ada, hammer).unwrap();
    let work = |w: &ServerWorld| -> u64 {
        w.world
            .bodies
            .iter()
            .map(|b| b.solver.iterations_total)
            .sum()
    };
    let mut most = 0;
    for _ in 0..3 {
        let taken = w.taken_since(0).to_vec();
        let near = about(&w, ada, 3.0, |l| {
            !taken.contains(&l.id) && matches!(l.what, Lying::Stone { size, .. } if size.x > 0.04)
        });
        #[allow(clippy::cast_precision_loss)]
        let now = w.tick as f64;
        w.apply(
            &Command::StrikeLoose {
                loose: near[0].id,
                started: now,
            },
            Some(ada),
        )
        .unwrap();
        for _ in 0..180 {
            let before = work(&w);
            w.steer(ada, &AvatarInput::default());
            w.step();
            most = most.max(work(&w).saturating_sub(before));
        }
        assert_eq!(
            w.world.bodies.len(),
            2,
            "all but the player and the hammerstone laid"
        );
    }
    // Measured 43 (the struck stone a body for its blow's second); as bodies, flakes took 250.
    assert!(most <= 50, "{most} Newton iterations in a tick");
}
