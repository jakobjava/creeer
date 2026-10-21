//! (G1.2e.) The player's own avatar predicted as it walks into things: crates in its path,
//! over a slow, jittery, lossy link. What the server knows and the client does not (a crate
//! it walked into) shows as a correction.

use super::tests::Game;
use crate::world::tests::world;
use creeer_math::glam::DVec3;
use creeer_net::link::Conditions;
use creeer_net::message::Command;
use std::time::Duration;

/// A player walks east at 3 m/s through a row of crates 3 m ahead (one each metre), over
/// 100–130 ms each way with one packet in twenty lost: its corrections.
pub(super) fn walk_into_crates() -> crate::predicted::Corrections {
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
    let link = Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    };
    game.join("ada", Some(link));
    game.run(90, |_| (DVec3::ZERO, false));
    game.run(240, |_| (east * 3.0, false));
    game.clients[0].0.corrections()
}

#[test]
#[ignore = "measures the corrections walking into crates"]
fn measure_walking_into_crates() {
    eprintln!("walking into crates: {:?}", walk_into_crates());
}
