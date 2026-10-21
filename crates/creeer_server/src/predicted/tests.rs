//! (G1.2e, ADR-0025.) The player's own avatar predicted as it walks into things: crates in
//! its path, with the bodies near it predicted too. What the server knows and the client does
//! not shows as a correction; the gate is corrections over 1 cm ≤ 1 % of them on a lossless
//! link (`docs/g1-plan.md`).

use crate::server::tests::Game;
use crate::world::tests::world;
use creeer_math::glam::DVec3;
use creeer_net::link::Conditions;
use creeer_net::message::Command;
use std::time::Duration;

/// 100–130 ms each way, one packet in twenty lost.
fn slow() -> Conditions {
    Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    }
}

/// A player walks east at 3 m/s through a row of six crates 3 m ahead (one each metre), over
/// `link` (none: lossless and at once).
fn walk_into_crates(link: Option<Conditions>) -> Game {
    let mut w = world();
    let up = w.spawn().normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    for k in 0..6 {
        let side = f64::from(k % 3) - 1.0;
        let at = w.ground_under(w.spawn() + east * (3.0 + f64::from(k)) + north * side * 0.6);
        w.apply(&Command::Crate(at + at.normalize() * 0.6), None)
            .unwrap();
    }
    let mut game = Game::new(w);
    game.join("ada", link);
    game.run(90, |_| (DVec3::ZERO, false));
    game.run(240, |_| (east * 3.0, false));
    game
}

#[test]
fn walking_into_crates_is_predicted() {
    // Measured (2026-10-21): lossless 1 of 163 over 1 cm, the largest 1.8 cm; slow and lossy
    // 5 of 148, the largest 5.5 cm. With the avatar alone predicted: 65 of 148, 38 cm.
    for (link, share, most) in [(None, 0.01, 0.03), (Some(slow()), 0.05, 0.1)] {
        let c = walk_into_crates(link).clients[0].0.corrections();
        assert!(c.count > 100, "{c:?}");
        assert!(
            f64::from(c.over_a_centimetre) <= share * f64::from(c.count),
            "{c:?}"
        );
        assert!(c.largest < most, "{c:?}");
    }
}

#[test]
fn the_crates_are_drawn_where_the_server_rests_them() {
    let mut game = walk_into_crates(Some(slow()));
    game.run(300, |_| (DVec3::ZERO, false));
    let client = &game.clients[0].0;
    let poses = client.poses();
    let world = &game.server.world;
    let mut resting = 0;
    for (id, body) in world.ids().iter().zip(&world.world.bodies) {
        if body.sleep.asleep && body.shapes.len() == 1 {
            resting += 1;
            let off = (poses[id].position - body.structure.position[0]).length();
            assert!(off < 0.01, "{id:?} drawn {off} m off");
        }
    }
    assert!(resting >= 6, "{resting} at rest");
}

#[test]
#[ignore = "measures the corrections walking into crates and the client's cost"]
fn measure_walking_into_crates() {
    for (name, link) in [("lossless", None), ("slow, lossy", Some(slow()))] {
        let c = walk_into_crates(link).clients[0].0.corrections();
        let each = c.replaying.as_secs_f64() * 1e3 / f64::from(c.count.max(1));
        eprintln!("{name}: {c:?}; {each:.3} ms a reconciliation");
    }
}
