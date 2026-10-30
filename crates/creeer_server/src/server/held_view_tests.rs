//! The user's case (G2.10, 2026-10-30: "moving the view around makes the character and the item
//! completly loose its mind"): a 0.95 m stick taken from the ground and placed as the client
//! places it in first person (`Connection::surface_aimed`, `Connection::place_hold`, the
//! client's `holding.rs`), the view sweeping round and up as the mouse turns it, then walked
//! with. It stays in the hands, within reach of the eyes, lying level, drawn where the server
//! has it, and never flung: it goes no faster than the hands steer it.

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

/// What happens to the stick over a game: the most it was beyond reach of the eyes (m), the
/// most its axis tilted from level (rad), its fastest (m/s), as drawn (from the avatar drawn) the
/// most it was off where the server has it (from its own avatar) and the most it moved in a frame
/// (m), the longest round trip (ticks), how far off it was drawn at rest at the end (m), and
/// whether it was held to the end.
#[derive(Debug, Default)]
struct Held {
    beyond: f64,
    tilt: f64,
    drawn_off: f64,
    jump: f64,
    trip: f64,
    at_rest: f64,
    fastest: f64,
    held: bool,
}

/// Ada, over `link`, takes the user's stick (spruce C24, 0.95 m, 46 mm: 0.67 kg) from the ground
/// a metre before her, looking down at it; turns her view 90° and up to level over a second (the
/// stick then before the eyes, meeting nothing), walks 3 m/s the way she looks for two, and turns
/// back walking for one, looking down again.
fn swept(link: Option<Conditions>) -> Held {
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
    let view_at = |t: f64| {
        let turned = (t.clamp(0.0, 1.0) - (t - 3.0).clamp(0.0, 1.0)) * std::f64::consts::FRAC_PI_2;
        let pitch = -0.8 * (1.0 - t.clamp(0.0, 1.0) + (t - 3.0).clamp(0.0, 1.0));
        let (s, c) = creeer_math::det::sin_cos(turned);
        let (ps, pc) = creeer_math::det::sin_cos(pitch);
        looking((east * c + north * s) * pc + up * ps, up)
    };
    let mut out = Held::default();
    let mut last: Option<DVec3> = None;
    for tick in 0..300u32 {
        let t = f64::from(tick) / 60.0;
        let view = view_at(t);
        let client = &mut game.clients[0].0;
        let surface = client.surface_aimed(view, |_, _, _| None);
        let hold = client.place_hold(view, surface, (0.0, 0.0, 0.0));
        client.set_hold(hold);
        let ahead = if (1.0..4.0).contains(&t) {
            (view * DVec3::NEG_Z).reject_from(up).normalize() * 3.0
        } else {
            DVec3::ZERO
        };
        game.tick(|_| (ahead, false));
        if game.server.world.carried(ada) != Some(id) {
            return out;
        }
        let load = game.server.world.index(id).expect("there");
        let s = &game.server.world.world.bodies[load].structure;
        let (p, q, v) = (s.position[0], s.orientation[0], s.velocity[0]);
        out.fastest = out.fastest.max(v.length());
        if t > 0.5 {
            let eye = reach::eye(game.avatar(0), up);
            out.beyond = out.beyond.max((p - eye).length() - reach::PLACING);
            let axis = q * DVec3::X;
            out.tilt = out
                .tilt
                .max(creeer_math::det::asin(axis.dot(up).abs().min(1.0)));
            // In the hands as drawn: from the avatar drawn, as the server has it from its own.
            let client = &game.clients[0].0;
            let me = client.avatar().expect("drawn").position;
            let drawn = client.poses()[&id].position - me;
            let off = (drawn - (p - game.avatar(0))).length();
            out.drawn_off = out.drawn_off.max(off);
            out.at_rest = off;
            out.jump = last.map_or(0.0, |l: DVec3| out.jump.max((drawn - l).length()));
            last = Some(drawn);
            let (newest, applied) = client.steered_inputs();
            out.trip = out.trip.max(f64::from(newest) - applied);
        }
    }
    out.held = game.server.world.carried(ada) == Some(id);
    out
}

/// Looked round, walked with, lossless and over 100 ms each way: held, within reach, level, no
/// faster than walking and the hands together; drawn moving no faster than the hands, off the
/// server's while gliding by no more than the hands go in a round trip, and where it is at rest.
/// Measured (2026-10-30, lossless): a frame moved it 48 mm at most; off by 122 mm gliding (the
/// round trip 6.5 ticks: 217 mm at the hands' speed), 0.4 mm at rest; slow, 49 mm, 401 mm (24
/// ticks: 791 mm) and 6 mm. Drawn at the newest hold
/// at once, it was 2.48 m off as the view swept from the ground to before the eyes.
#[test]
fn a_stick_held_in_first_person_stays_put_as_the_view_sweeps_and_walks() {
    for link in [None, Some(slow())] {
        let held = swept(link);
        eprintln!("{}: {held:?}", link.map_or("lossless", |_| "slow"));
        assert!(held.held && held.beyond < 0.05, "{held:?}");
        assert!(held.tilt < 0.1 && held.fastest < 6.0, "{held:?}");
        let gliding = creeer_sim::carry::FASTEST * held.trip / 60.0 + 0.05;
        assert!(held.drawn_off < gliding && held.at_rest < 0.02, "{held:?}");
        assert!(held.jump < 0.07, "{held:?}");
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
