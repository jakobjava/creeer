//! Walking and stopping with something heavy in the hands, as the player plays it (G2.11, A4;
//! stream B found it walking a 27 kg pole up to a trench: asked 1.9–3 m/s she went 3.8–4.1,
//! and stopping at the bank she slid on 1.4–2.2 m and fell in).

use super::builder::{Builder, looking, pole};
use creeer_math::glam::DVec3;

/// Ada carries what she holds (if anything) east at `speed` for 2 s, then lets go of the keys
/// for 2 s, looking level the way she walks and holding it as the client holds it: her fastest
/// over the ground (m/s) and how far she went after letting go (m), as the server has her.
fn walk_and_stop(b: &mut Builder, speed: f64) -> (f64, f64) {
    let view = looking(b.east, b.up);
    let up = b.up;
    let speed_now = |b: &Builder| {
        let w = &b.game.server.world;
        let i = w.index(b.ada).expect("there");
        w.world.bodies[i].structure.velocity[0]
            .reject_from(up)
            .length()
    };
    let mut fastest = 0.0f64;
    let mut stopped_at = None;
    for tick in 0..240 {
        let walk = if tick < 120 {
            b.east * speed
        } else {
            DVec3::ZERO
        };
        if tick == 120 {
            stopped_at = Some(b.game.avatar(0));
        }
        let turned = b.taken_turn(view);
        let client = &mut b.game.clients[0].0;
        if b.game.server.world.carried(b.ada).is_some() {
            let surface = client.surface_aimed(view, |_, _, _| None);
            let hold = client.place_hold(view, surface, (turned.0, turned.1, 0.0), b.asked);
            b.asked = hold.map(|h| h.turn);
            client.set_hold(hold);
        }
        b.game.tick(|_| (walk, false));
        fastest = fastest.max(speed_now(b));
    }
    let went = (b.game.avatar(0) - stopped_at.expect("stopped")).reject_from(up);
    (fastest, went.length())
}

/// A post stood up in its hole, set (P) while held, and held through its 8 s of backfilling, as
/// the player sets one (`build_flow_tests.rs`): its holder should stay where she stood, within
/// 2 cm. She slides back about 0.3 m: the walk's damper alone holds a standing body, and gives
/// way to the steady pull of the hands. Soles holding her where she stands mended it, and moved
/// B's building tests and the prediction's gates (ADR-0042, Measured and rejected).
#[test]
#[ignore = "fails: a holder slides back 0.3 m holding through work (ADR-0042, open)"]
fn holding_a_post_through_its_backfilling_its_holder_stays_put() {
    let mut b = Builder::new();
    let mouth = b.ground(1.3, 0.0);
    b.dig(mouth, 0.6);
    let east = b.east;
    let post = b.lay(pole(1.6, 0.05), b.ground(0.0, -1.2), east);
    b.game.run(60, super::still);
    b.take(post);
    let standing = (0.0, std::f64::consts::FRAC_PI_2, 0.0);
    b.hold_at(mouth, standing, 150);
    let before = b.game.avatar(0);
    b.command(post, mouth, creeer_net::message::Command::SetPost);
    b.hold_at(mouth, standing, 9 * 60);
    let slid = (b.game.avatar(0) - before).reject_from(b.up).length();
    assert!(
        b.game.server.world.carried(b.ada).is_none(),
        "set, let go: {:?}",
        b.told()
    );
    assert!(slid < 0.02, "slid {slid:.3} m holding it");
}

#[test]
#[ignore = "measures walking and stopping with a 27 kg pole: by hand"]
fn measure_walking_and_stopping_with_a_heavy_pole() {
    for speed in [1.4, 3.0] {
        let mut empty = Builder::new();
        let (fastest, went) = walk_and_stop(&mut empty, speed);
        eprintln!("empty-handed at {speed}: fastest {fastest:.2} m/s, went {went:.2} m stopping");
        // (Lying across the way, so it is carried across it, not pointing ahead into the ground.)
        let mut b = Builder::new();
        let north = b.north;
        let p = b.lay(pole(3.0, 0.06), b.ground(0.6, 0.0), north);
        b.game.run(60, super::still);
        b.take(p);
        let (fastest, went) = walk_and_stop(&mut b, speed);
        eprintln!(
            "with the pole at {speed}: fastest {fastest:.2} m/s, went {went:.2} m stopping, held {}",
            b.game.server.world.carried(b.ada).is_some()
        );
    }
}
