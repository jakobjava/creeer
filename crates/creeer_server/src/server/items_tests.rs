//! What a client is told of items (G1.6): a made tool's body as a tool drawn by its pieces, and a
//! flake lying on the ground as a flake: never by the shape the physics gives it.

use crate::server::tests::{Game, still};
use crate::world::tests::world;
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::items::{Edge, Form, Item, Piece};

#[test]
fn a_client_knows_a_tool_as_a_tool_and_a_flake_as_a_flake() {
    let mut game = Game::new(world());
    game.join("ada", None);
    game.run(60, still);
    let w = &mut game.server.world;
    let up = w.spawn().normalize();
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let mut flake = Piece::solid(
        "chert",
        2580.0,
        Form::Lump {
            size: DVec3::new(0.04, 0.03, 0.008),
        },
    );
    flake.edge = Some(Edge {
        angle: 0.5,
        length: 0.04,
        radius: 5e-6,
    });
    let haft = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Rod {
            radius: 0.015,
            length: 0.6,
        },
    );
    let strip = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(0.9, 0.03, 0.0015),
        },
    );
    let made = creeer_sim::making::bind(haft, flake.clone(), strip, 20e6).unwrap();
    let at = w.ground_under(w.spawn() + east * 3.0) + up * 0.2;
    let tool = w.add_item(Item::Made(Box::new(made)), at, true);
    let lying = w.lay(
        Item::Piece(flake, Vec::new()),
        w.spawn() - east * 3.0,
        DQuat::IDENTITY,
    );
    game.run(120, still);
    let client = &game.clients[0].0;
    let creeer_net::message::ItemLook { word, parts, .. } =
        client.items.get(&tool).expect("told what the tool is");
    assert_eq!((word.as_str(), parts.len()), ("tool", 2));
    let laid = client.laid.get(&lying).expect("told what lies there");
    assert_eq!(laid.word, "flake");
}

/// Walking with a made tool in hand, over the real client: the avatar walks as asked, keeps its
/// tool, and its prediction keeps with the server; a short haft and a branch 1.6 m long (turned
/// into the hand at once, a tool spun on the hands' spring and a long one struck the ground,
/// knocking its holder down: the user, "one can not walk anymore").
#[test]
fn one_walks_with_a_made_tool_in_hand() {
    for (length, radius, strip) in [(0.6, 0.015, 0.6), (1.6, 0.02, 1.6)] {
        walks_with((length, radius), strip);
    }
}

fn walks_with((length, radius): (f64, f64), strip: f64) {
    let mut game = Game::new(world());
    game.join("ada", None);
    game.run(60, still);
    let ada = game.clients[0].0.welcome.as_ref().unwrap().avatar;
    let haft = Piece::solid("spruce_c24", 420.0, Form::Rod { radius, length });
    let head = Piece::solid(
        "chert",
        2580.0,
        Form::Lump {
            size: DVec3::new(0.12, 0.07, 0.03),
        },
    );
    let strip = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(strip, 0.06, 0.0015),
        },
    );
    let made = creeer_sim::making::bind(haft, head, strip, 20e6).unwrap();
    game.server
        .world
        .put_in_hands(ada, Item::Made(Box::new(made)))
        .unwrap();
    game.run(90, still);
    let up = game.avatar(0).normalize();
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let (from, before) = (game.avatar(0), game.clients[0].0.corrections().count);
    game.run(180, |_| (east * 3.0, false));
    let went = (game.avatar(0) - from).reject_from(up).length();
    let c = game.clients[0].0.corrections();
    let drawn = game.clients[0].0.avatar().unwrap().position;
    println!(
        "{length} m: went {went:.2} m; corrections {} (over 1 cm {}), largest {:.3} m; drawn {:.3} m off",
        c.count - before,
        c.over_a_centimetre,
        c.largest,
        (drawn - game.avatar(0)).length()
    );
    assert!(game.server.world.carried(ada).is_some(), "still in hand");
    assert!(went > 7.0, "walked {went} m in 3 s at 3 m/s");
    assert!(
        (drawn - game.avatar(0)).length() < 0.2,
        "the prediction kept up"
    );
}
