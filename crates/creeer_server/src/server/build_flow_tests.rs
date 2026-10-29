//! Building as the player builds (G2.10, the user's play, 2026-10-29: "i cant get things to
//! fasten"): a pole taken from the ground, turned with the view, held out over another lying
//! there, lowered by looking down until it touches, and fastened with J where it touches, all
//! through the client as the game steers it (`Connection::carry_hold`) and its commands.

use super::Game;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::message::{Command, Target};
use creeer_sim::items::{Form, Item, Piece};

/// A view looking along `forward` (a camera's turn: −z ahead, y up), level with `up`.
fn looking(forward: DVec3, up: DVec3) -> DQuat {
    let right = forward.cross(up).normalize();
    DQuat::from_mat3(&DMat3::from_cols(right, right.cross(forward), -forward)).normalize()
}

/// A green spruce pole, 50 mm round and 1.6 m long, lying along `along`.
fn pole() -> Item {
    let rod = Form::Rod {
        radius: 0.025,
        length: 1.6,
    };
    Item::Piece(Piece::solid("spruce_green", 800.0, rod), Vec::new())
}

/// Ada builds with `fastener` (creative: nails and strips at hand): returns how many seconds
/// the two poles took to touch, and whether they became one structure, the hands then free.
fn build(fastener: &str, seconds: u32) -> (f64, bool, Vec<String>) {
    let mut w = crate::world::tests::world();
    w.creative = true;
    let mut game = Game::new(w);
    game.join("ada", None);
    game.run(120, super::still);
    let centre = game.avatar(0);
    let up = centre.normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let lay = |game: &mut Game, at: DVec3, axis: DVec3| {
        let w = &mut game.server.world;
        let id = w.add_item(pole(), w.ground_under(centre + at) + up * 0.03, true);
        let i = w.index(id).expect("there");
        let b = &mut w.world.bodies[i];
        let side = up.reject_from(axis).normalize();
        b.structure.orientation[0] =
            DQuat::from_mat3(&DMat3::from_cols(axis, side, axis.cross(side)));
        b.solver.reset_history(&b.structure);
        id
    };
    // The one to take a metre south, lying east–west; the other ahead to the east and a little
    // to the left, lying east–west, its near end 0.4 m ahead.
    let taken = lay(&mut game, -north * 1.0, east);
    let lying = lay(&mut game, east * 1.2 + north * 0.35, east);
    game.run(60, super::still);
    let client = &mut game.clients[0].0;
    let drawn = client.poses()[&taken].position;
    let aimed = client.aim(drawn + up * 3.0, -up, 5.0).expect("the pole");
    client.command(Command::Grab(Target {
        body: aimed.body,
        at: aimed.point,
        seen: aimed.seen,
    }));
    // Taken looking at it, south and down; its turn kept from the heading (across it).
    let (south, ahead) = (-north, east);
    let view_at = |t: f64| {
        let turned = (t / 0.6).clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2;
        let (s, c) = creeer_math::det::sin_cos(turned);
        let level = south * c + ahead * s;
        // Looking down more and more from the second second on: the hands lower it.
        let pitch = -0.5 - 0.5 * ((t - 1.0) / 2.0).clamp(0.0, 1.0);
        let (ps, pc) = creeer_math::det::sin_cos(pitch);
        looking(level * pc + up * ps, up)
    };
    game.run(30, super::still);
    let turn = game.clients[0]
        .0
        .held_in_view(view_at(0.0))
        .expect("held")
        .1;
    let ada = game.clients[0].0.welcome.as_ref().expect("joined").avatar;
    assert_eq!(game.server.world.carried(ada), Some(taken), "taken");
    let (mut touched, mut fastening) = (None, false);
    for tick in 0..seconds * 60 {
        let t = f64::from(tick) / 60.0;
        let client = &mut game.clients[0].0;
        // Held out a little beyond the chest, as with the wheel.
        let hold = client.carry_hold(view_at(t), turn, 0.3);
        client.set_hold(hold);
        game.tick(super::still);
        let w = &game.server.world;
        let (Some(a), Some(b)) = (w.index(taken), w.index(lying)) else {
            continue;
        };
        if std::env::var("TRACE").is_ok() && tick % 15 == 0 {
            let me = w.world.bodies[w.index(ada).expect("ada")]
                .structure
                .position[0];
            let rel = |p: DVec3| [(p - me).dot(east), (p - me).dot(north), (p - me).dot(up)];
            let (pa, pb) = (
                w.world.bodies[a].structure.position[0],
                w.world.bodies[b].structure.position[0],
            );
            eprintln!("t {t:.2}: held {:.2?} lying {:.2?}", rel(pa), rel(pb));
        }
        let touch =
            (w.world.touching(a, 0.005).into_iter()).find(|c| c.other.map(|o| o.0) == Some(b));
        if let (Some(touch), false) = (touch, fastening) {
            touched = Some(t);
            fastening = true;
            #[allow(clippy::cast_precision_loss)]
            let seen = w.tick as f64;
            game.clients[0].0.command(Command::Fasten {
                target: Target {
                    body: Some(taken),
                    at: touch.point,
                    seen,
                },
                fastener: fastener.into(),
            });
        }
    }
    let w = &mut game.server.world;
    let one = w.index(lying).is_none() && w.index(taken).is_some();
    let free = w.carried(ada).is_none();
    let notes = w.take_notes().into_iter().map(|n| n.1).collect();
    (touched.unwrap_or(f64::NAN), one && free, notes)
}

#[test]
fn a_pole_held_over_another_is_lowered_onto_it_and_nailed() {
    let (touched, built, notes) = build("nail_100", 12);
    eprintln!("nailed: touched at {touched:.2} s, built {built}, {notes:?}");
    assert!(touched < 4.0, "touched at {touched} s: {notes:?}");
    assert!(built, "{notes:?}");
}

#[test]
fn a_pole_held_over_another_is_lowered_onto_it_and_lashed() {
    let (touched, built, notes) = build("bast_lashing", 40);
    eprintln!("lashed: touched at {touched:.2} s, built {built}, {notes:?}");
    assert!(touched < 4.0, "touched at {touched} s: {notes:?}");
    assert!(built, "{notes:?}");
}
