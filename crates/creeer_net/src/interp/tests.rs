//! Interpolated entities follow their true paths between snapshots, run on briefly past the
//! newest, and the playback clock settles behind the server.

use super::*;
use crate::snapshot::Sender;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.81;

/// A crate thrown at (8, 6) m/s from (100, 50) m, under gravity, spinning: its state at `t`.
fn thrown(t: f64) -> BodyState {
    let v = DVec3::new(8.0, 6.0 - G * t, 0.0);
    let p = DVec3::new(100.0 + 8.0 * t, 50.0 + 6.0 * t - 0.5 * G * t * t, 0.0);
    let spin = DVec3::new(0.0, 0.0, 2.0);
    BodyState::new(p, DQuat::from_scaled_axis(spin * t), (v, spin), false)
}

fn received(ticks: impl Iterator<Item = u32>) -> Receiver {
    let (mut sender, mut receiver) = (Sender::default(), Receiver::default());
    for tick in ticks {
        let state = thrown(f64::from(tick) * DT);
        let bytes = sender.write(tick, 0, &[(EntityId(1), state, 1.0)], (usize::MAX, &[]));
        receiver.read(&bytes).expect("a snapshot");
        sender.ack(tick);
    }
    receiver
}

#[test]
fn between_snapshots_a_thrown_crate_keeps_its_parabola() {
    // Snapshots every other tick, one of them lost.
    let receiver = received((2..40).step_by(2).filter(|&t| t != 20));
    let mut worst = (0.0f64, 0.0f64);
    for k in 0..300 {
        let tick = 4.0 + f64::from(k) * 0.1;
        let pose = sample(&receiver, tick, DT)[&EntityId(1)];
        let truth = thrown(tick * DT);
        worst.0 = worst.0.max((pose.position - truth.position()).length());
        worst.1 = worst
            .1
            .max(pose.orientation.angle_between(truth.orientation()));
    }
    // A quadratic path is exactly Hermite's: only the quantisation is left (1 mm, 4 mm/s
    // over 33–67 ms); the spin is uniform, the arc exact.
    assert!(worst.0 < 2e-3, "{} m off", worst.0);
    assert!(worst.1 < 2e-4, "{} rad off", worst.1);
}

#[test]
fn past_the_newest_snapshot_it_runs_on_a_little_then_waits() {
    let receiver = received([10, 12].into_iter());
    let at_newest = sample(&receiver, 12.0, DT)[&EntityId(1)].position;
    let a_bit_later = sample(&receiver, 15.0, DT)[&EntityId(1)].position;
    let much_later = sample(&receiver, 100.0, DT)[&EntityId(1)].position;
    let v = thrown(12.0 * DT).velocity();
    assert!((a_bit_later - at_newest - v * 3.0 * DT).length() < 1e-9);
    assert!((much_later - at_newest - v * EXTRAPOLATE * DT).length() < 1e-9);
}

#[test]
fn the_playback_clock_settles_behind_the_server() {
    let mut playback = Playback::new(4.0);
    let mut drawn = 0.0;
    // Packets every other tick, arriving with jitter: the server's tick as they tell it.
    for frame in 0..600u32 {
        let server = f64::from(frame) * 0.5; // 120 frames a second, 60 ticks
        let jitter = f64::from((frame * 7) % 5) - 2.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let newest = ((server + jitter).max(0.0) as u32) & !1;
        drawn = playback.advance(0.5, Some(newest));
    }
    let server = 599.0 * 0.5;
    assert!(
        (drawn - (server - 4.0)).abs() < 2.5,
        "drawn at {drawn} for {server}"
    );
}
