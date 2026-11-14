//! What a client is told of parts fastened into a structure (G2.11 A5): fastened while nobody
//! held them (a play scene's builders, or the part aimed at while another is held), the
//! structure kept the first part's entity and the clients were never told it had become one:
//! they drew it as the lone part it was and predicted it so (its partner gone, it fell where the
//! partner had been and was rewound at every snapshot: *log_bridge*'s client 382 Newton
//! iterations a tick, and a rewind at every snapshot for two seconds after).

use crate::server::tests::{Game, still};
use creeer_math::glam::DQuat;
use creeer_net::entity::{EntityId, Kind};
use creeer_net::message::{Command, Target};
use creeer_sim::items::{Form, Item, Piece};

/// A green spruce pole, 1.6 m long, 5 cm across (*builders*').
fn pole() -> Item {
    let rod = Form::Rod {
        radius: 0.025,
        length: 1.6,
    };
    Item::Piece(Piece::solid("spruce_green", 800.0, rod), Vec::new())
}

#[test]
fn parts_fastened_unheld_are_told_as_the_structure_they_became() {
    let mut w = crate::world::tests::world();
    w.creative = true;
    let spawn = w.spawn();
    let up = spawn.normalize();
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let ground = w.ground_under(spawn + east * 1.0);
    let lower = w.add_item(pole(), ground + up * 0.03, true);
    let upper = w.add_item(pole(), ground + up * 0.09, true);
    let i = w.index(upper).expect("there");
    let s = &mut w.world.bodies[i].structure;
    s.orientation[0] = DQuat::from_axis_angle(up, std::f64::consts::FRAC_PI_2) * s.orientation[0];
    let mut game = Game::new(w);
    game.join("ada", None);
    game.run(120, still);
    let w = &game.server.world;
    let (a, b) = (
        w.index(upper).expect("upper"),
        w.index(lower).expect("lower"),
    );
    let at = (w.world.touching(a, 0.005).into_iter())
        .find(|t| t.other.is_some_and(|(j, _)| j == b))
        .map(|t| t.point)
        .expect("crossed");
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    let target = Target {
        body: Some(upper),
        at,
        seen,
    };
    game.clients[0].0.command(Command::Fasten {
        target,
        fastener: "nail_100".into(),
    });
    game.run(240, still);
    let notes = game.server.world.take_notes();
    let w = &game.server.world;
    assert!(w.index(lower).is_none(), "nailed into one: {notes:?}");
    let kinds = &game.clients[0].0.kinds;
    assert!(
        matches!(kinds.get(&upper), Some(Kind::Structure(_))),
        "told as a structure: {:?}",
        kinds.get(&upper).map(std::mem::discriminant)
    );
    assert!(!kinds.contains_key(&lower), "the other part gone");
    // Drawn where the server has it, and not predicted as a lone body.
    let drawn = game.clients[0].0.poses()[&upper].position;
    let there = w.world.bodies[w.index(upper).expect("there")]
        .structure
        .position[0];
    assert!((drawn - there).length() < 0.01, "{drawn} against {there}");
    let predicted = game.clients[0].0.predicted.as_ref().expect("predicting");
    let ids: Vec<EntityId> = predicted.nearby_motions().iter().map(|m| m.0).collect();
    assert!(!ids.contains(&upper), "{ids:?}");
}
