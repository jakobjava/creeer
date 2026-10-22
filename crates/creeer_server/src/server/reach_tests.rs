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

#[test]
fn a_strike_over_a_slow_link_is_seen_by_the_others_and_its_blow_falls() {
    let (mut game, centre, up, east) = joined();
    let ground = game.server.world.ground_under(centre + east * 1.3);
    (game.server.world)
        .apply(&Command::Crate(ground + up * 0.55), None)
        .unwrap();
    let id = *game.server.world.ids().last().unwrap();
    game.clients[0].0.command(Command::Wield { tool: Some(2) });
    game.run(60, still);
    // She aims at the crate's top and strikes.
    let client = &mut game.clients[0].0;
    let drawn = client.poses()[&id].position;
    let aimed = (client.aim(drawn + up * 3.0, -up, 5.0)).expect("the crate in her view");
    client.strike(aimed);
    let ada = client.welcome.as_ref().unwrap().avatar;
    assert!(client.swinging(ada).is_some(), "her own swing at once");
    let mine = game.clients[0].0.swings[&ada];
    // A second on (the blow), and her word of it on its way: struck.
    let mut blows = Vec::new();
    let mut ticks = 0;
    while blows.is_empty() && ticks < 150 {
        game.run(1, still);
        ticks += 1;
        blows = game.server.world.take_blows();
    }
    assert_eq!(blows.len(), 1, "the blow fell");
    assert_eq!(blows[0].on, Some(id));
    // Bob was told when her swing began, as of his snapshots' ticks.
    // Begun at her own tick if her word came in time, or when it came: never before.
    let told = game.clients[1]
        .0
        .swings
        .get(&ada)
        .copied()
        .expect("Bob saw her swing");
    assert!(told >= mine, "begun at {told}, her own {mine}");
    println!("struck {ticks} ticks after she asked; begun at {told}, her own tick {mine}");
}

/// Carrying a 20 kg box and walking off with it over the slow and lossy link: the box, held,
/// is told to the client as a joint's side (not predicted on its own), and the client's own
/// prediction of its avatar, which knows nothing of the load, is still rarely corrected (the
/// gates of the prediction's own tests on such a link: at most 5 % over 1 cm, the largest under
/// 10 cm).
#[test]
fn walking_with_a_load_over_a_slow_link_the_avatar_is_rarely_corrected() {
    let (mut game, centre, up, east) = joined();
    let at = game.server.world.ground_under(centre + east * 1.0) + up * 0.21;
    let shape = creeer_physics::world::shape::Shape::Box {
        half: DVec3::splat(0.2),
    };
    let mut body = creeer_physics::world::Body::rigid(shape, 20.0, at, crate::world::DT);
    body.friction = 0.6;
    let id = game.server.world.add_body(body, None);
    game.run(30, still);
    let client = &mut game.clients[0].0;
    let drawn = client.poses()[&id].position;
    let aimed = client.aim(drawn + up * 3.0, -up, 5.0).expect("the box");
    let target = creeer_net::message::Target {
        body: aimed.body,
        at: aimed.point,
        seen: aimed.seen,
    };
    client.command(Command::Grab(target));
    game.run(90, still);
    let ada = game.clients[0].0.welcome.as_ref().unwrap().avatar;
    assert_eq!(game.server.world.carried(ada), Some(id));
    let before = game.clients[0].0.corrections();
    let walking = |k: usize| (if k == 0 { east * 1.2 } else { DVec3::ZERO }, false);
    let mut largest: f64 = 0.0;
    for _ in 0..180 {
        let last = game.clients[0].0.corrections().largest;
        game.run(1, walking);
        let now = game.clients[0].0.corrections().largest;
        if now > last {
            largest = largest.max(now);
        }
    }
    let after = game.clients[0].0.corrections();
    assert_eq!(game.server.world.carried(ada), Some(id), "still carried");
    let (count, over) = (
        after.count - before.count,
        after.over_a_centimetre - before.over_a_centimetre,
    );
    println!(
        "walking with the load: {over} of {count} corrections over 1 cm, the largest new {largest:.3} m"
    );
    assert!(
        f64::from(over) <= 0.05 * f64::from(count),
        "{over} of {count} over 1 cm"
    );
    assert!(largest < 0.1, "{largest} m");
}
