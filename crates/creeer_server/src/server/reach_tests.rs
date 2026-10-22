//! A player reaches for what they see, over a slow and lossy link (G1.4d). A body predicted
//! with the avatar is drawn at the avatar's own tick, a little ahead of the server's; another
//! is drawn a little in the past, so by the time the server hears of it it has moved on, and
//! the server, looking it up as it was when seen, finds the point on it and where it has gone.

use super::tests::{Game, still};
use crate::world::tests::world;
use creeer_math::glam::DVec3;
use creeer_net::link::Conditions;
use creeer_net::message::Command;
use creeer_sim::{character, reach};
use std::time::Duration;

/// Ada over a slow and lossy link and Bob, both joined and standing, Ada's avatar's centre and
/// up, and the ground's way east.
fn joined() -> (Game, DVec3, DVec3, DVec3) {
    let mut game = Game::new(world());
    let link = Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    };
    game.join("ada", Some(link));
    game.join("bob", None);
    game.run(120, still);
    let centre = game.avatar(0);
    let up = character::plumb(&game.server.world.world, centre);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    (game, centre, up, east)
}

#[allow(clippy::cast_precision_loss)]
fn now(game: &Game) -> f64 {
    game.server.world.tick as f64
}

#[test]
fn a_falling_crate_predicted_with_her_is_reached_as_it_is() {
    let (mut game, centre, up, east) = joined();
    (game.server.world)
        .apply(&Command::Crate(centre + east * 1.2 + up), None)
        .unwrap();
    let id = *game.server.world.ids().last().unwrap();
    game.run(14, still);
    // She aims down at its top: predicted with her avatar, at her own tick.
    let client = &game.clients[0].0;
    let drawn = client.poses()[&id].position;
    let aimed = (client.aim(drawn + up * 3.0, -up, 5.0)).expect("the crate in her view");
    assert_eq!(aimed.body, Some(id));
    let avatar = client.welcome.as_ref().unwrap().avatar;
    let mine = client.avatar().unwrap().position;
    assert!(
        reach::within(mine, up, aimed.point),
        "within her reach as she sees it"
    );
    game.run(8, still);
    // Seen ahead of the server (her inputs wait in its jitter buffer): judged as it is now.
    let seen = game.server.world_tick(aimed.seen);
    assert!(
        seen > now(&game) - 4.0,
        "seen at {seen}, now {}",
        now(&game)
    );
    let reached = (game.server.world)
        .reached(avatar, Some(id), aimed.point, seen)
        .expect("reached as predicted");
    assert!((reached.point - aimed.point).length() < 0.05);
}

#[test]
fn a_walking_player_aimed_at_over_a_slow_link_is_reached_as_he_was_seen() {
    let (mut game, _, up, east) = joined();
    let walking = |k: usize| (if k == 1 { east * 1.4 } else { DVec3::ZERO }, false);
    game.run(10, walking);
    // She aims at his front, as she draws him: in the past.
    let bob = game.clients[1].0.welcome.as_ref().unwrap().avatar;
    let client = &game.clients[0].0;
    let drawn = client.poses()[&bob].position;
    let aimed = (client.aim(drawn + east * 2.0, -east, 3.0)).expect("Bob in her view");
    assert_eq!(aimed.body, Some(bob));
    let avatar = client.welcome.as_ref().unwrap().avatar;
    let mine = client.avatar().unwrap().position;
    assert!(
        reach::within(mine, up, aimed.point),
        "within her reach as she sees it"
    );
    // Her word of it on its way while he walks on.
    game.run(8, walking);
    let seen = game.server.world_tick(aimed.seen);
    let behind = now(&game) - seen;
    assert!(
        behind >= 12.0,
        "seen {behind} ticks before the server hears of it"
    );
    let w = &game.server.world;
    let reached = (w.reached(avatar, Some(bob), aimed.point, seen)).expect("reached as seen");
    assert_eq!(reached.body, Some((w.index(bob).unwrap(), 0)));
    let on = (reached.point - aimed.point).dot(east);
    // (He sets off from standing: measured 0.14 m.)
    assert!(
        on > 0.1,
        "the point went on {on} m with him since she saw it"
    );
    // Judged where he is now, not where she saw him, it would be refused.
    let why = w
        .reached(avatar, Some(bob), aimed.point, now(&game))
        .unwrap_err();
    assert!(why.contains("not on the body"), "{why}");
}
