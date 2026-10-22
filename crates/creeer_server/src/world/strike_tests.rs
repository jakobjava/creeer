//! A player strikes what they reach with the tool in hand (G1.4e): the blow falls a second after
//! the swing starts, an impulse as the collision of the tool's and the body's effective masses
//! says, one swing a cycle.

use super::strikes::Blow;
use super::tests::world;
use super::*;
use creeer_net::message::Target;
use creeer_sim::strike;

/// A world with ada standing (a second steered) holding the axe, a crate on the ground beside
/// her, her avatar and the crate.
fn axe_and_crate() -> (ServerWorld, EntityId, EntityId) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = character::plumb(&w.world, centre);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let ground = w.ground_under(centre + east * 1.3);
    w.apply(&Command::Crate(ground + up * 0.55), None).unwrap();
    let id = *w.ids().last().unwrap();
    w.apply(&Command::Wield { tool: Some(2) }, Some(ada))
        .unwrap();
    wait(&mut w, ada, 60);
    (w, ada, id)
}

fn wait(w: &mut ServerWorld, ada: EntityId, n: u64) {
    for _ in 0..n {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

/// The top of the crate's face towards her, as it is now.
fn on_crate(w: &ServerWorld, ada: EntityId, id: EntityId) -> Target {
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = character::plumb(&w.world, centre);
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    Target {
        body: Some(id),
        at: s.position[0] + up * 0.5,
        seen,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn ticks(seconds: f64) -> u64 {
    (seconds / DT).round() as u64
}

#[test]
fn an_axe_blow_falls_a_second_on_and_moves_the_crate_as_a_collision_does() {
    let (mut w, ada, id) = axe_and_crate();
    let target = on_crate(&w, ada, id);
    w.apply(
        &Command::Strike {
            target,
            started: now(&w),
        },
        Some(ada),
    )
    .unwrap();
    assert_eq!(w.take_swings_started(), vec![(ada, w.tick)]);
    // Nothing until the blow.
    wait(&mut w, ada, ticks(strike::TO_BLOW) - 1);
    assert!(w.take_blows().is_empty());
    let i = w.index(id).unwrap();
    let before = w.world.bodies[i].structure.velocity[0];
    wait(&mut w, ada, 1);
    let blows: Vec<Blow> = w.take_blows();
    assert_eq!(blows.len(), 1, "one blow");
    let blow = blows[0];
    assert_eq!((blow.by, blow.tool, blow.on), (ada, 2, Some(id)));
    // The impulse: the axe (17 m/s, 1.45 kg at its edge) on the 200 kg crate.
    let swing = strike::swing(&w.tools[2]);
    let way = blow.impulse.normalize();
    let crate_mass = w.world.effective_mass(i, 0, blow.at, way);
    let restitution = w.world.bodies[i].restitution;
    let want = strike::impulse(swing.speed, swing.mass, crate_mass, restitution);
    assert!((blow.impulse.length() - want).abs() < 1e-9);
    // Down on its top: the head's way is downward there.
    let up = blow.at.normalize();
    assert!(way.dot(up) < -0.5, "the blow's way {way}");
    // Its momentum changed by the impulse (the blow falls after the step).
    let after = w.world.bodies[i].structure.velocity[0];
    let change = (after - before) * 200.0;
    assert!(
        (change - blow.impulse).length() < 1e-6 * want,
        "{change} against {}",
        blow.impulse
    );
    println!(
        "axe on a crate: {:.1} N·s, {:.0} J",
        blow.impulse.length(),
        blow.energy
    );
}

#[test]
fn one_swing_a_cycle_and_only_with_a_tool_in_hand_standing() {
    let (mut w, ada, id) = axe_and_crate();
    let target = |w: &ServerWorld| on_crate(w, ada, id);
    w.apply(
        &Command::Strike {
            target: target(&w),
            started: now(&w),
        },
        Some(ada),
    )
    .unwrap();
    wait(&mut w, ada, 10);
    let why = w
        .apply(
            &Command::Strike {
                target: target(&w),
                started: now(&w),
            },
            Some(ada),
        )
        .unwrap_err();
    assert!(why.contains("still swinging"), "{why}");
    wait(&mut w, ada, ticks(strike::CYCLE));
    w.apply(
        &Command::Strike {
            target: target(&w),
            started: now(&w),
        },
        Some(ada),
    )
    .unwrap();
    // Empty-handed: none.
    wait(&mut w, ada, ticks(strike::CYCLE));
    w.apply(&Command::Wield { tool: None }, Some(ada)).unwrap();
    let why = w
        .apply(
            &Command::Strike {
                target: target(&w),
                started: now(&w),
            },
            Some(ada),
        )
        .unwrap_err();
    assert!(why.contains("no tool"), "{why}");
    // Not a player's.
    assert!(
        w.apply(
            &Command::Strike {
                target: target(&w),
                started: now(&w)
            },
            None
        )
        .is_err()
    );
}

#[test]
fn a_blow_on_the_ground_keeps_its_energy_for_digging() {
    let (mut w, ada, _) = axe_and_crate();
    let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = character::plumb(&w.world, centre);
    let (_, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let at = w.ground_under(centre + north * 1.5);
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    let target = Target {
        body: None,
        at,
        seen,
    };
    w.apply(
        &Command::Strike {
            target,
            started: now(&w),
        },
        Some(ada),
    )
    .unwrap();
    wait(&mut w, ada, ticks(strike::TO_BLOW));
    let blows = w.take_blows();
    assert_eq!(blows.len(), 1);
    assert_eq!((blows[0].on, blows[0].impulse), (None, DVec3::ZERO));
    let energy = strike::swing(&w.tools[2]).energy;
    assert!((blows[0].energy - energy).abs() < 1e-9 && energy > 200.0);
}

#[test]
fn a_crate_gone_out_of_reach_before_the_blow_is_missed() {
    let (mut w, ada, id) = axe_and_crate();
    w.apply(
        &Command::Strike {
            target: on_crate(&w, ada, id),
            started: now(&w),
        },
        Some(ada),
    )
    .unwrap();
    // Carried off 5 m along the ground while the axe is raised.
    let i = w.index(id).unwrap();
    let up = w.world.bodies[i].structure.position[0].normalize();
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let at = w.world.bodies[i].structure.position[0] + east * 5.0;
    w.world.bodies[i].structure.position[0] = w.ground_under(at) + up * 0.55;
    wait(&mut w, ada, ticks(strike::TO_BLOW));
    assert!(w.take_blows().is_empty());
}

/// The world's tick now, as a strike gives it.
#[allow(clippy::cast_precision_loss)]
fn now(w: &ServerWorld) -> f64 {
    w.tick as f64
}

#[test]
fn a_swing_begins_at_the_players_own_tick_but_never_before_now() {
    let (mut w, ada, id) = axe_and_crate();
    // Begun three ticks ahead (her inputs waiting here): its blow falls three ticks later.
    let target = on_crate(&w, ada, id);
    let ahead = Command::Strike {
        target,
        started: now(&w) + 3.0,
    };
    w.apply(&ahead, Some(ada)).unwrap();
    assert_eq!(w.take_swings_started(), vec![(ada, w.tick + 3)]);
    wait(&mut w, ada, ticks(strike::TO_BLOW) + 2);
    assert!(w.take_blows().is_empty());
    wait(&mut w, ada, 1);
    assert_eq!(w.take_blows().len(), 1);
    // Claimed in the past (to skip the raise): begun now. Far ahead: the buffer's most.
    wait(&mut w, ada, ticks(strike::CYCLE));
    let target = on_crate(&w, ada, id);
    let past = Command::Strike {
        target,
        started: now(&w) - 50.0,
    };
    w.apply(&past, Some(ada)).unwrap();
    assert_eq!(w.take_swings_started(), vec![(ada, w.tick)]);
    wait(&mut w, ada, ticks(strike::CYCLE));
    let target = on_crate(&w, ada, id);
    let far = Command::Strike {
        target,
        started: now(&w) + 500.0,
    };
    w.apply(&far, Some(ada)).unwrap();
    let most = u64::try_from(creeer_net::predict::MOST_BUFFER).unwrap();
    assert_eq!(w.take_swings_started(), vec![(ada, w.tick + most)]);
}
