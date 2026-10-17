//! Snapshots rebuild on the client exactly what the server meant, cost nothing for what did
//! not change, survive lost packets, and share a budget fairly.

use super::*;
use creeer_math::glam::{DQuat, DVec3};

fn state(x: f64, turn: f64, asleep: bool) -> BodyState {
    BodyState::new(
        DVec3::new(250_000.0 + x, 12.0, -40_000.0),
        DQuat::from_rotation_y(turn),
        (DVec3::new(x, 0.0, 0.0), DVec3::ZERO),
        asleep,
    )
}

fn view(states: &[BodyState]) -> Vec<(EntityId, BodyState, f64)> {
    #[allow(clippy::cast_possible_truncation)] // a few
    states
        .iter()
        .enumerate()
        .map(|(i, s)| (EntityId(i as u32 * 3 + 1), *s, 1.0))
        .collect()
}

fn expected(view: &[(EntityId, BodyState, f64)]) -> States {
    view.iter().map(|(id, s, _)| (*id, *s)).collect()
}

#[test]
fn the_client_rebuilds_what_the_server_sent() {
    let (mut sender, mut receiver) = (Sender::default(), Receiver::default());
    let bodies: Vec<BodyState> = (0..20)
        .map(|i| state(f64::from(i), 0.1 * f64::from(i), i % 3 == 0))
        .collect();
    let v = view(&bodies);
    let bytes = sender.write(2, 7, &v, usize::MAX);
    let got = receiver.read(&bytes).expect("a snapshot");
    assert_eq!(got.tick, 2);
    assert_eq!(got.last_input, 7);
    assert_eq!(got.states, expected(&v));
}

#[test]
fn what_did_not_change_costs_nothing_and_what_moved_little_costs_little() {
    let (mut sender, mut receiver) = (Sender::default(), Receiver::default());
    let mut bodies: Vec<BodyState> = (0..100).map(|i| state(f64::from(i), 0.0, true)).collect();
    let first = sender.write(2, 0, &view(&bodies), usize::MAX);
    receiver.read(&first).expect("first");
    sender.ack(2);
    // Nothing changed: a header and no entries.
    let same = sender.write(4, 0, &view(&bodies), usize::MAX);
    assert!(same.len() <= 8, "{} bytes for nothing", same.len());
    assert_eq!(
        receiver.read(&same).expect("same").states,
        expected(&view(&bodies))
    );
    sender.ack(4);
    // One body moved a centimetre (awake now): a few bytes.
    bodies[40] = state(40.01, 0.0, false);
    let moved = sender.write(6, 0, &view(&bodies), usize::MAX);
    assert!(moved.len() <= 20, "{} bytes for a centimetre", moved.len());
    assert!(first.len() > 100 * 20, "{} bytes in all", first.len());
    assert_eq!(
        receiver.read(&moved).expect("moved").states,
        expected(&view(&bodies))
    );
}

#[test]
fn lost_snapshots_and_late_acknowledgements_lose_nothing() {
    let (mut sender, mut receiver) = (Sender::default(), Receiver::default());
    let mut bodies: Vec<BodyState> = (0..10).map(|i| state(f64::from(i), 0.0, false)).collect();
    let first = sender.write(2, 0, &view(&bodies), usize::MAX);
    receiver.read(&first).expect("first");
    sender.ack(2);
    // Two snapshots lost on the way: each written against tick 2, the newest acknowledged.
    for tick in [4, 6] {
        bodies[3] = state(3.0 + f64::from(tick), 0.0, false);
        let _lost = sender.write(tick, 0, &view(&bodies), usize::MAX);
    }
    bodies[5] = state(-5.0, 1.0, false);
    let third = sender.write(8, 0, &view(&bodies), usize::MAX);
    assert_eq!(
        receiver.read(&third).expect("third").states,
        expected(&view(&bodies))
    );
    // A body that left view is removed; one that came in arrives whole.
    sender.ack(8);
    let mut v = view(&bodies);
    v.remove(2);
    v.push((EntityId(1000), state(9.0, 2.0, true), 1.0));
    let fourth = sender.write(10, 0, &v, usize::MAX);
    assert_eq!(receiver.read(&fourth).expect("fourth").states, expected(&v));
}

#[test]
fn a_snapshot_without_its_baseline_is_dropped_and_the_next_recovers() {
    let (mut sender, mut receiver) = (Sender::default(), Receiver::default());
    let bodies: Vec<BodyState> = (0..5).map(|i| state(f64::from(i), 0.0, false)).collect();
    let _never_arrived = sender.write(2, 0, &view(&bodies), usize::MAX);
    sender.ack(2); // a client that lies, or a late duplicate acknowledgement
    let moved: Vec<BodyState> = (0..5)
        .map(|i| state(f64::from(i) + 0.5, 0.0, false))
        .collect();
    let next = sender.write(4, 0, &view(&moved), usize::MAX);
    assert!(receiver.read(&next).is_err(), "its baseline never arrived");
    // The client acknowledges nothing it has: the server's acknowledged tick stays, but a
    // fresh sender (or one past its history) writes whole states.
    let mut fresh = Sender::default();
    let whole = fresh.write(6, 0, &view(&moved), usize::MAX);
    assert_eq!(
        receiver.read(&whole).expect("whole").states,
        expected(&view(&moved))
    );
}

#[test]
fn a_budget_shares_the_bytes_and_every_body_gets_its_turn() {
    let (mut sender, mut receiver) = (Sender::default(), Receiver::default());
    let budget = 1024;
    let mut last_sent = vec![0u32; 300];
    for round in 1..=40u32 {
        let tick = 2 * round;
        // Every body moves every tick.
        let bodies: Vec<BodyState> = (0..300)
            .map(|i| {
                state(
                    f64::from(i) + 0.01 * f64::from(round),
                    0.001 * f64::from(round),
                    false,
                )
            })
            .collect();
        let v = view(&bodies);
        let bytes = sender.write(tick, 0, &v, budget);
        assert!(bytes.len() <= budget, "{} bytes", bytes.len());
        let got = receiver.read(&bytes).expect("a snapshot").states.clone();
        sender.ack(tick);
        for (i, (id, s, _)) in v.iter().enumerate() {
            if got.get(id) == Some(s) {
                last_sent[i] = round;
            }
        }
        if round >= 20 {
            // No body waits for long: each was brought up to date within the last 20.
            let stale = last_sent.iter().filter(|&&r| round - r >= 20).count();
            assert_eq!(stale, 0, "round {round}");
        }
    }
}
