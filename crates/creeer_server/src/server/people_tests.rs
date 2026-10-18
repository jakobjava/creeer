//! A player's character fallen, as the player sees it: told it is down and how hurt, its
//! ragdoll drawn as the server has it (its own prediction paused), and standing again,
//! predicted afresh where it got up.

use super::tests::Game;
use crate::world::tests::world;
use creeer_math::glam::DVec3;
use creeer_net::entity::Kind;

fn still(_: usize) -> (DVec3, bool) {
    (DVec3::ZERO, false)
}

#[test]
fn a_player_sees_their_character_fall_and_stand_up_again() {
    let mut game = Game::new(world());
    game.join("ada", None);
    game.run(90, still);
    let avatar = game.clients[0].0.welcome.as_ref().unwrap().avatar;
    // Dropped from 10 m.
    let w = &mut game.server.world;
    let i = w.index(avatar).unwrap();
    let body = &mut w.world.bodies[i];
    let up = body.structure.position[0].normalize();
    body.structure.position[0] += up * 10.0;
    body.solver.reset_history(&body.structure);
    let mut ticks = 0;
    while !game.clients[0].0.fallen {
        game.run(1, still);
        ticks += 1;
        assert!(ticks < 300, "never told it fell");
    }
    game.run(30, still);
    let client = &game.clients[0].0;
    assert!(client.health < 0.9, "health {}", client.health);
    assert!(
        matches!(client.kinds[&avatar], Kind::Body(_)),
        "drawn as its character"
    );
    let segments = client
        .kinds
        .values()
        .filter(|k| matches!(k, Kind::Body(_)))
        .count();
    assert_eq!(segments, 15);
    // Drawn where the server has its pelvis (two ticks behind, as everything else).
    let off = (client.avatar().unwrap().position - game.avatar(0)).length();
    assert!(off < 0.3, "its ragdoll drawn {off} m off");
    // Up again: a character, predicted, where the server has it.
    let mut ticks = 0;
    while game.clients[0].0.fallen {
        game.run(1, still);
        ticks += 1;
        assert!(ticks < 1200, "down for 20 s");
    }
    game.run(60, still);
    let client = &game.clients[0].0;
    assert!(matches!(&client.kinds[&avatar], Kind::Character { player } if player == "ada"));
    assert_eq!(client.kinds.len(), 1, "its segments still drawn");
    let off = (client.avatar().unwrap().position - game.avatar(0)).length();
    assert!(off < 0.05, "predicted {off} m off");
}
