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

/// The placements of the crates the measures run over: walking into a row of crates is chaotic
/// (a centimetre moves a correction by centimetres), so each is a sample of an ensemble.
const PLACEMENTS: u32 = 8;

/// A player walks east at 3 m/s through a row of six crates about 3 m ahead (one each metre),
/// over `link` (none: lossless and at once); `placement` (0..[`PLACEMENTS`]) moves the row a
/// little, its crates 0.56-0.63 m apart across and 3.00-3.14 m ahead.
fn walk_into_crates(link: Option<Conditions>, placement: u32) -> Game {
    let (across, ahead) = (
        0.56 + 0.01 * f64::from(placement),
        3.0 + 0.02 * f64::from(placement),
    );
    let mut w = world();
    let up = w.spawn().normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    for k in 0..6 {
        let side = f64::from(k % 3) - 1.0;
        let at = w.ground_under(w.spawn() + east * (ahead + f64::from(k)) + north * side * across);
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
    // Gated over the ensemble of placements (G1.3i, as Baker et al. 2015 test a chaotic model by
    // an ensemble): the median and the worst share of corrections over 1 cm, and the largest
    // correction, each a tenth above what was measured at the spawn moved to a temperate
    // latitude (2026-10-22): lossless 0-4 of 163 over 1 cm (median 2), the largest 2.9 cm;
    // slow and lossy 5-25 of 148 (median 16), the largest 12.8 cm. The old spawn, by the pole,
    // measured lossless 0-1 and slow 3-10 (median 6.5): on level ground the character's box
    // meets upright crates face to face, a contact a quantum of tilt turns (ADR-0025, amended).
    // G2.10, an impact's budget counting the joints before the step (ADR-0005): every impact in
    // a group with the person's legs had stopped the whole group dead, the crates struck
    // included, alike on the server and the client. They now fly on as struck: lossless 1-9 of
    // 163 over 1 cm (median 1.5, the largest 2.6 cm), the worst share 0.0245 → 0.0552; slow
    // 13-28 of 148 (the largest 9.1 cm), the median share 0.108 → 0.141. Those two gates a tenth
    // above (0.027 → 0.061, 0.12 → 0.156); the rest are not moved.
    for (link, median, worst, largest) in [
        (None, 0.014, 0.061, 0.032),
        (Some(slow()), 0.156, 0.19, 0.14),
    ] {
        let mut shares = Vec::new();
        for placement in 0..PLACEMENTS {
            let c = walk_into_crates(link, placement).clients[0].0.corrections();
            assert!(c.count > 100, "{placement}: {c:?}");
            assert!(c.largest < largest, "{placement}: {c:?}");
            shares.push(f64::from(c.over_a_centimetre) / f64::from(c.count));
        }
        shares.sort_by(f64::total_cmp);
        let n = shares.len();
        let middle = 0.5 * (shares[n / 2 - 1] + shares[n / 2]);
        assert!(
            middle <= median && shares[n - 1] <= worst,
            "{link:?}: {shares:?}"
        );
    }
}

#[test]
fn the_crates_are_drawn_where_the_server_rests_them() {
    let mut game = walk_into_crates(Some(slow()), 4);
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
        for placement in 0..PLACEMENTS {
            let c = walk_into_crates(link, placement).clients[0].0.corrections();
            let each = c.replaying.as_secs_f64() * 1e3 / f64::from(c.count.max(1));
            eprintln!("{name} {placement}: {c:?}; {each:.3} ms a reconciliation");
        }
    }
}

impl super::Predicted {
    /// The bodies predicted with the avatar (for probes).
    pub(crate) fn nearby_count(&self) -> usize {
        self.nearby.len()
    }

    /// The inputs not yet applied by the server (for probes).
    pub(crate) fn pending(&self) -> usize {
        self.log.pending().count()
    }
}
