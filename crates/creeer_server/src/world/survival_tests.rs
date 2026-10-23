//! The survival start (G1.6b, `docs/g1-plan.md` principle 9): a player has the clothes they wear
//! and nothing else, no jetpack and no tools; dead, they start again at the spawn with nothing,
//! what they carried left where they fell, and nothing refilled. A creative server hands the
//! jetpack and its refill out.

use super::tests::world;
use super::*;

fn at(w: &ServerWorld, id: EntityId) -> DVec3 {
    w.world.bodies[w.index(id).unwrap()].structure.position[0]
}

fn wait(w: &mut ServerWorld, ada: EntityId, n: u32, input: &AvatarInput) {
    for _ in 0..n {
        w.steer(ada, input);
        w.step();
    }
}

fn survival() -> (ServerWorld, EntityId) {
    let mut w = world();
    w.creative = false;
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60, &AvatarInput::default());
    (w, ada)
}

#[test]
fn a_survival_player_has_no_jetpack_and_no_tools() {
    let (mut w, ada) = survival();
    let mass = w.world.bodies[w.index(ada).unwrap()].structure.masses()[0];
    assert!((mass - character::DRY).abs() < 1e-12, "{mass}");
    assert!(w.gas(ada).abs() < 1e-12);
    // Thrust asked for does nothing: no lift.
    let up = character::plumb(&w.world, at(&w, ada));
    let before = at(&w, ada);
    let thrust = AvatarInput {
        thrust: up,
        stopping: false,
        ..AvatarInput::default()
    };
    wait(&mut w, ada, 60, &thrust);
    assert!((at(&w, ada) - before).dot(up) < 0.01);
    let why = w
        .apply(&Command::Wield { tool: Some(0) }, Some(ada))
        .unwrap_err();
    assert!(why.contains("make"), "{why}");
}

#[test]
fn dead_a_player_starts_again_with_nothing_and_leaves_what_they_carried() {
    let (mut w, ada) = survival();
    // A pebble in a pocket (dropped by the host: survival has no kit).
    let up = character::plumb(&w.world, at(&w, ada));
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let where_ = w.ground_under(at(&w, ada) + east * 0.8) + up * 0.3;
    w.apply(
        &Command::Kit {
            item: 1,
            at: where_,
        },
        None,
    )
    .unwrap();
    let pebble = *w.ids().last().unwrap();
    wait(&mut w, ada, 60, &AvatarInput::default());
    let weight = w.world.bodies[w.index(pebble).unwrap()].structure.masses()[0];
    let top = at(&w, pebble) + up * 0.015;
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    let aim = creeer_net::message::Target {
        body: Some(pebble),
        at: top,
        seen,
    };
    w.apply(&Command::Stow(Some(aim)), Some(ada)).unwrap();
    assert!((w.load(ada) - weight).abs() < 1e-15);
    let died_at = at(&w, ada);
    w.people.get_mut(&ada).unwrap().health = 0.0;
    let mut ticks = 0;
    while w.person(ada).is_some_and(|p| p.health <= 0.0) && ticks < 2000 {
        wait(&mut w, ada, 1, &AvatarInput::default());
        ticks += 1;
    }
    assert!(w.person(ada).is_some_and(|p| p.health > 0.0), "respawned");
    assert!(w.load(ada) == 0.0, "nothing in the pockets");
    let mass = w.world.bodies[w.index(ada).unwrap()].structure.masses()[0];
    assert!(
        (mass - character::DRY).abs() < 1e-9,
        "nothing refilled: {mass}"
    );
    // The pebble lies where they fell.
    let left = (w.items.bodies.keys())
        .filter_map(|id| {
            w.index(*id)
                .map(|i| w.world.bodies[i].structure.position[0])
        })
        .any(|p| (p - died_at).length() < 2.0);
    assert!(left, "what they carried is where they fell");
}

#[test]
fn a_creative_player_has_the_jetpack_and_it_is_refilled() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    assert!((w.gas(ada) - character::TANK).abs() < 1e-9);
}
