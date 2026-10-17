//! Islands are found and numbered alike however the edges come, ledgers close to rounding over
//! millions of entries, exchanges lose nothing between rates, and islands sleep and wake.

use super::*;

#[test]
fn islands_are_found_and_numbered_whatever_the_order_of_the_edges() {
    let edges = [(0, 3), (5, 6), (3, 1), (7, 5), (2, 2)];
    let a = Islands::new(9, edges);
    let mut reversed = edges;
    reversed.reverse();
    let b = Islands::new(9, reversed.iter().map(|&(x, y)| (y, x)));
    assert_eq!(a, b);
    // {0, 1, 3}, {2}, {4}, {5, 6, 7}, {8}: numbered by lowest node.
    assert_eq!(a.count(), 5);
    assert_eq!(a.members(0), &[0, 1, 3]);
    assert_eq!(a.island_of(7), 3);
    assert_eq!(a.local(7), 2);
    assert_eq!(a.members(4), &[8]);
}

#[test]
fn a_compensated_sum_does_not_drift() {
    // Ten million tenths, then a large term and its negative: a plain sum is off by ~1e-4.
    let (mut sum, mut plain) = (Sum::default(), 0.0f64);
    for _ in 0..10_000_000 {
        sum.add(0.1);
        plain += 0.1;
    }
    sum.add(1e15);
    sum.add(-1e15);
    assert!((sum.value() - 1e6).abs() < 1e-9, "{}", sum.value() - 1e6);
    assert!(
        (plain - 1e6).abs() > 1e-7,
        "a plain sum drifts ({})",
        plain - 1e6
    );
}

#[test]
fn a_ledger_closes_when_energy_is_conserved() {
    // A battery (1 kJ stored) drives a heater through a wire, 10,000 steps.
    let mut stored = 1000.0;
    let mut ledger = Ledger::new(stored);
    for _ in 0..10_000 {
        let out = 0.01;
        stored -= out;
        ledger.dissipate(0.3 * out);
        ledger.export(0.7 * out); // the heater's heat, to the thermal network
    }
    assert!(
        ledger.relative_imbalance(stored) < 1e-12,
        "{}",
        ledger.imbalance(stored)
    );
    // A network that makes 1 mJ from nothing is caught.
    assert!((ledger.imbalance(stored + 1e-3) - 1e-3).abs() < 1e-10);
}

#[test]
fn an_exchange_between_rates_loses_nothing() {
    let mut exchange = Exchange::default();
    let (fast, slow) = (Schedule::EVERY_TICK, Schedule { every: 3, phase: 1 });
    let (mut given, mut taken) = (Sum::default(), Sum::default());
    for tick in 0..1000u64 {
        if fast.due(tick) {
            #[allow(clippy::cast_precision_loss)]
            let joules = 0.25 + (tick % 7) as f64 * 0.01;
            exchange.give(joules);
            given.add(joules);
        }
        if slow.due(tick) {
            taken.add(exchange.take());
        }
    }
    taken.add(exchange.take());
    assert!((given.value() - taken.value()).abs() < 1e-12);
    assert!(slow.due(4) && !slow.due(5) && (slow.step(1.0 / 60.0) - 0.05).abs() < 1e-15);
}

#[test]
fn an_island_sleeps_when_still_and_wakes_when_disturbed() {
    let mut sleep = Sleep::default();
    for step in 0..sleep::STILL_STEPS {
        let asleep = sleep.after_step(1e-9, 1e-6);
        assert_eq!(asleep, step + 1 == sleep::STILL_STEPS);
    }
    assert!(sleep.asleep());
    sleep.wake();
    assert!(!sleep.asleep());
    assert!(!sleep.after_step(1.0, 1e-6));
}
