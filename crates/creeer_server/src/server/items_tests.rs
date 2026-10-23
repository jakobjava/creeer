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
