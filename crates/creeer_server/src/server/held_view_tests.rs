//! The user's case (G2.10, 2026-10-29: "things move around when you move the mouse (float
//! away), things are not held in the hand ... the arm go through the body"): a 0.95 m stick
//! taken from the ground and steered as the client steers it (`Connection::hold_in_view`, the
//! client's `holding.rs`), the view turning and pitching as the mouse turns it, then walked
//! with. It stays in the hands: within the arms' reach of the stooped shoulders, never torn
//! from them, lying as it was taken, and drawn where the server has it.

use super::Game;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::link::Conditions;
use creeer_net::message::{Command, Target};
use creeer_sim::items::{Form, Item, Piece};
use creeer_sim::reach;
use std::time::Duration;

/// A view looking along `forward` (a camera's turn: −z ahead, y up), level with `up`.
fn looking(forward: DVec3, up: DVec3) -> DQuat {
    let right = forward.cross(up).normalize();
    DQuat::from_mat3(&DMat3::from_cols(right, right.cross(forward), -forward)).normalize()
}

/// What happens to the stick over a game: the most its grip was beyond the arms' reach (m), the
/// most its axis tilted from level (rad), the most its grip was drawn beyond the reach of the
/// avatar (m), and
/// whether it was held to the end.
#[derive(Debug, Default)]
struct Held {
    beyond: f64,
    tilt: f64,
    drawn_off: f64,
    held: bool,
    /// The least it was held before the body's axis the way the view heads (m).
    ahead: f64,
}

/// Ada, over `link`, takes the user's stick (spruce C24, 0.95 m, 46 mm: 0.67 kg) from the ground
/// a metre before her, turns her view 90° and up to level over a second, walks 3 m/s the way
/// she looks for two, and turns back walking for one.
fn steered(link: Option<Conditions>, carried: bool) -> Held {
    let mut game = Game::new(crate::world::tests::world());
    game.join("ada", link);
    game.run(120, super::still);
    let centre = game.avatar(0);
    let up = centre.normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let rod = Form::Rod {
        radius: 0.023,
        length: 0.95,
    };
    let stick = Item::Piece(Piece::solid("spruce_c24", 420.0, rod), Vec::new());
    let at = game.server.world.ground_under(centre + east * 1.0) + up * 0.03;
    let id = game.server.world.add_item(stick, at, true);
    game.run(60, super::still);
    let client = &mut game.clients[0].0;
    let drawn = client.poses()[&id].position;
    let aimed = client.aim(drawn + up * 3.0, -up, 5.0).expect("the stick");
    client.command(Command::Grab(Target {
        body: aimed.body,
        at: aimed.point,
        seen: aimed.seen,
    }));
    game.run(30, super::still);
    let ada = game.clients[0].0.welcome.as_ref().expect("joined").avatar;
    assert_eq!(game.server.world.carried(ada), Some(id), "taken");
    // Looking down at it, east, as the player did to take it.
    let view_at = |t: f64| {
        let turned = (t.clamp(0.0, 1.0) - (t - 3.0).clamp(0.0, 1.0)) * std::f64::consts::FRAC_PI_2;
        let pitch = -0.8 * (1.0 - t.clamp(0.0, 1.0));
        let (s, c) = creeer_math::det::sin_cos(turned);
        let (ps, pc) = creeer_math::det::sin_cos(pitch);
        looking((east * c + north * s) * pc + up * ps, up)
    };
    let start = view_at(0.0);
    let mut kept = game.clients[0].0.held_in_view(start).expect("drawn");
    let mut out = Held {
        ahead: f64::INFINITY,
        ..Held::default()
    };
    for tick in 0..240u32 {
        let t = f64::from(tick) / 60.0;
        let view = view_at(t);
        let client = &mut game.clients[0].0;
        let hold = if carried {
            let hold = client.carry_hold(view, kept.1).expect("held");
            (hold, kept.0)
        } else {
            client.hold_in_view(view, kept, false).expect("held")
        };
        kept.0 = hold.1;
        client.set_hold(Some(hold.0));
        let ahead = if (1.0..4.0).contains(&t) {
            (view * DVec3::NEG_Z).reject_from(up).normalize() * 3.0
        } else {
            DVec3::ZERO
        };
        game.tick(|_| (ahead, false));
        let Some(load) = game.server.world.index(id) else {
            break;
        };
        if game.server.world.carried(ada) != Some(id) {
            return out;
        }
        let centre = game.avatar(0);
        let s = &game.server.world.world.bodies[load].structure;
        let (p, q) = (s.position[0], s.orientation[0]);
        let carried = game.clients[0].0.own_carried().expect("told");
        let grip = p + q * carried.grip;
        if t > 0.5 {
            let reachable = reach::within_arms(centre, up, grip);
            out.beyond = out.beyond.max((grip - reachable).length());
            let forward = (view * DVec3::NEG_Z).reject_from(up).normalize();
            out.ahead = out.ahead.min((grip - centre).dot(forward));
            let axis = q * DVec3::X;
            out.tilt = out
                .tilt
                .max(creeer_math::det::asin(axis.dot(up).abs().min(1.0)));
            // In the hands as drawn: the load drawn where it is steered (ahead of the server's,
            // whose hands follow at their plan's pace), its grip within the arms' reach of the
            // avatar as drawn (predicted).
            let client = &game.clients[0].0;
            let me = client.avatar().expect("drawn").position;
            let pose = client.poses()[&id];
            let grip = pose.position + pose.orientation * carried.grip;
            let reachable = reach::within_arms(me, me.normalize(), grip);
            out.drawn_off = out.drawn_off.max((grip - reachable).length());
        }
    }
    out.held = game.server.world.carried(ada) == Some(id);
    out
}

/// Placed (left Alt held): where the view aims from the eyes, before the body and within the
/// arms' reach. Measured (2026-10-29), lossless and over 100 ms each way: the grip never beyond
/// the arms' reach on the server; the stick tilted at most 1.3° and 1.1° from level; its grip
/// drawn at most 3.8 cm and 20.8 cm beyond the reach of the avatar as drawn (the own load drawn
/// where it is steered, its model of the server's hands without their plan's lead). Steered as
/// before, from the camera 4.5 m behind the avatar (the user's case): tilted 28° (it stood up
/// as the view came level) and drawn 66 cm beyond the hands' reach, the arm through the body.
#[test]
fn a_stick_placed_with_the_view_stays_in_the_hands() {
    let lossless = steered(None, false);
    eprintln!("placed, lossless: {lossless:?}");
    assert!(
        lossless.held && lossless.beyond < 0.01 && lossless.tilt < 0.05,
        "{lossless:?}"
    );
    assert!(
        lossless.drawn_off < 0.06 && lossless.ahead > 0.2,
        "{lossless:?}"
    );
    let slow = steered(Some(slow()), false);
    eprintln!("placed, slow: {slow:?}");
    assert!(
        slow.held && slow.beyond < 0.01 && slow.tilt < 0.05 && slow.drawn_off < 0.25,
        "{slow:?}"
    );
}

/// Carried (the user, 2026-10-29: "things are usually carried in front of the chest and not at
/// some random position close to the body (like behind the back)"): the stick goes with the body
/// before the chest the way the view heads, whatever the eyes look at, the view turning and the
/// body walking.
#[test]
fn a_stick_carried_goes_with_the_body_before_the_chest() {
    for link in [None, Some(slow())] {
        let carried = steered(link, true);
        eprintln!(
            "carried, {}: {carried:?}",
            link.map_or("lossless", |_| "slow")
        );
        assert!(carried.held && carried.beyond < 0.01, "{carried:?}");
        // Before the torso's front (its box 0.16 m before its axis): measured at least 0.52 m
        // ahead lossless, 0.18 m over the slow link as the view turns (the hold a round trip
        // late), level within 1.7°.
        assert!(carried.tilt < 0.05 && carried.ahead > 0.15, "{carried:?}");
    }
}

/// 100–130 ms each way, one packet in twenty lost.
fn slow() -> Conditions {
    Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    }
}
