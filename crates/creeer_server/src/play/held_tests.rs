//! A play scene's player carries and lays a part by hand (G2.11 B2: `Act::hold`), as a client
//! steers it: at the bridge site, a deck board taken from its row and laid on the ground before
//! her, its hold placed each tick as the client's `holding.rs` places it, and recorded.

use super::Play;
use super::record::Act;
use super::scenes::{terrain, world};
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::message::{Command, Target};

/// A view looking along `forward`, level with `up` (a camera's turn: −z ahead, y up).
fn looking(forward: DVec3, up: DVec3) -> DQuat {
    let right = forward.cross(up).normalize();
    DQuat::from_mat3(&DMat3::from_cols(right, right.cross(forward), -forward)).normalize()
}

#[test]
fn a_scenes_player_carries_and_lays_a_part_by_hand() {
    let terrain = terrain();
    let mut w = world(std::sync::Arc::clone(&terrain));
    w.creative = true;
    let site = w.prepare_bridge_site();
    let board = site.item("deck board", 11).expect("the row's last board");
    let mut play = Play::new(w, "bridge_demo", &["ada"], &terrain);
    let up = play.avatar(0).normalize();
    // To the board's east side, taken.
    let lies = play.server.world.world.bodies[play.server.world.index(board).expect("there")]
        .structure
        .position[0];
    for _ in 0..600 {
        let way = (lies + site.across * 0.9 - play.avatar(0)).reject_from(up);
        if way.length() < 0.2 {
            break;
        }
        play.tick(&[Act::walk(way.normalize() * way.length().clamp(0.5, 2.0))]);
    }
    play.tick(&[Act::default()]);
    let aimed = (play.client(0).aim(lies + up * 3.0, -up, 5.0)).expect("the board");
    let take = Act {
        commands: vec![Command::Grab(Target {
            body: aimed.body,
            at: aimed.point,
            seen: aimed.seen,
        })],
        ..Act::default()
    };
    play.tick(&[take]);
    for _ in 0..30 {
        play.tick(&[Act::default()]);
    }
    assert_eq!(
        play.server
            .world
            .carried(play.client(0).welcome.as_ref().expect("joined").avatar),
        Some(board)
    );
    // Laid on the ground 1.5 m south of her, across the view, as the client places it.
    let to = play
        .server
        .world
        .ground_under(play.avatar(0) - site.along * 1.5);
    let mut asked = None;
    for _ in 0..240 {
        let c = play.client(0);
        let eye = creeer_sim::reach::eye(c.avatar().expect("drawn").position, up);
        let view = looking((to - eye).normalize(), up);
        let hold = c.place_hold(
            view,
            c.surface_aimed(view, |_, _, _| None),
            (0.0, 0.0, 0.0),
            asked,
        );
        asked = hold.map(|h| h.turn);
        play.tick(&[Act {
            hold,
            ..Act::default()
        }]);
    }
    play.tick(&[Act {
        commands: vec![Command::Drop],
        ..Act::default()
    }]);
    for _ in 0..60 {
        play.tick(&[Act::default()]);
    }
    let w = &play.server.world;
    let now = w.world.bodies[w.index(board).expect("there")]
        .structure
        .position[0];
    let off = (now - to).reject_from(up).length();
    println!("laid {off:.3} m from where aimed");
    assert!(off < 0.3, "laid where aimed: {off} m off");
    // Its recording keeps the holds as steered (a replay of it is not yet the same state bit for
    // bit: `docs/future-improvements.md`).
    let held = (play.recording().ticks.iter())
        .filter(|t| t[0].hold.is_some())
        .count();
    assert_eq!(held, 240, "every steered tick's hold recorded");
}
