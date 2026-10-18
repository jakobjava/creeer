//! Stale rates, many networks side by side, and the same answer however it is advanced.

use crate::{Event, Network, Process, ProcessId, Stock, StockId, advance_all};

fn drain(level: f64, trust_from: f64) -> (Network, StockId) {
    let mut n = Network::new(0.0);
    let tank = n.add_stock(Stock::new("tank", level, 0.0, 1000.0));
    let out = n.add_stock(Stock::unbounded("out"));
    let mut p = Process::constant("drain", &[(tank, -2.0), (out, 2.0)]);
    p.trust = vec![(tank, trust_from, 1000.0)];
    n.add_process(p);
    (n, tank)
}

#[test]
fn advancing_stops_where_a_process_goes_stale() {
    let (mut n, tank) = drain(100.0, 20.0);
    let events = n.advance(1000.0);
    assert_eq!(events.len(), 1, "{events:?}");
    let Event::Stale { process, time } = events[0] else {
        panic!("{events:?}")
    };
    assert_eq!(process, ProcessId(0));
    assert!(
        (time - 40.0).abs() < 1e-9 && (n.time() - 40.0).abs() < 1e-9,
        "{time}"
    );
    assert!((n.stock(tank).amount - 20.0).abs() < 1e-9);
    // Measured again (trusted lower), it runs on to empty.
    let mut fresh = n.process(ProcessId(0)).clone();
    fresh.trust = vec![(tank, 0.0, 1000.0)];
    n.set_process(ProcessId(0), fresh);
    let events = n.advance(1000.0);
    assert!(
        matches!(events[..], [Event::Empty { time, .. }] if (time - 50.0).abs() < 1e-9),
        "{events:?}"
    );
    // Outside its box from the start: stale at once.
    let (mut low, _) = drain(10.0, 20.0);
    assert!(matches!(low.advance(5.0)[..], [Event::Stale { time, .. }] if time == 0.0));
    assert_eq!(low.time(), 0.0);
}

#[test]
fn time_advanced_in_steps_or_at_once_gives_the_same() {
    let build = || {
        let mut n = Network::new(0.0);
        let a = n.add_stock(Stock::new("a", 50.0, 0.0, 60.0));
        let b = n.add_stock(Stock::new("b", 0.0, 0.0, 30.0));
        let sink = n.add_stock(Stock::unbounded("sink"));
        n.add_process(Process::constant("a→b", &[(a, -1.5), (b, 1.5)]));
        n.add_process(Process::constant("b→", &[(b, -1.0), (sink, 1.0)]));
        n
    };
    let (mut once, mut steps) = (build(), build());
    once.advance(200.0);
    for k in 1..=200 {
        steps.advance(f64::from(k));
    }
    for k in 0..3 {
        let (x, y) = (
            once.stock(StockId(k)).amount,
            steps.stock(StockId(k)).amount,
        );
        assert!(
            (x - y).abs() < 1e-9 * x.abs().max(1.0),
            "{k}: {x} against {y}"
        );
    }
}

#[test]
fn networks_side_by_side_are_each_as_alone() {
    let mut many: Vec<Network> = (0..64)
        .map(|k| drain(100.0 + f64::from(k), 0.0).0)
        .collect();
    let mut alone = many.clone();
    let events = advance_all(&mut many, 3600.0);
    for (k, (n, a)) in many.iter().zip(&mut alone).enumerate() {
        let e = a.advance(3600.0);
        assert_eq!(events[k], e);
        assert_eq!(
            n.stock(StockId(0)).amount.to_bits(),
            a.stock(StockId(0)).amount.to_bits()
        );
    }
}

/// A tank fed 2/s that drains k·level fills towards 2/k; with a cap below that it is full at
/// t = −ln(1 − k·cap/2)/k exactly (the exact segments find the crossing).
#[test]
fn a_level_dependent_tank_fills_to_its_cap_at_the_analytic_time() {
    let k = 0.01;
    let mut n = Network::new(0.0);
    let src = n.add_stock(Stock::unbounded("src"));
    let tank = n.add_stock(Stock::new("tank", 0.0, 0.0, 150.0));
    let out = n.add_stock(Stock::unbounded("out"));
    n.add_process(Process::constant("feed", &[(src, -2.0), (tank, 2.0)]));
    n.add_process(Process {
        name: "outlet".into(),
        flows: vec![
            crate::Flow {
                stock: tank,
                rate: 0.0,
                slope: vec![(tank, -k, 0.0)],
            },
            crate::Flow {
                stock: out,
                rate: 0.0,
                slope: vec![(tank, k, 0.0)],
            },
        ],
        trust: Vec::new(),
    });
    let events = n.advance(1000.0);
    let exact = -creeer_math::det::ln(1.0 - k * 150.0 / 2.0) / k;
    let full = events.iter().find_map(|e| match *e {
        Event::Full { time, .. } => Some(time),
        _ => None,
    });
    let full = full.expect("full");
    eprintln!(
        "full at {full} s (exact {exact}), {} segments",
        n.segments()
    );
    assert!(
        (full - exact).abs() < 1e-9 * exact,
        "{full} against {exact}"
    );
    // Held full after: the feed throttled to the outflow (2 − 1.5 = 0.5 spare).
    assert!((n.stock(tank).amount - 150.0).abs() < 1e-9);
    let (into, outof) = n.books(tank);
    assert!(n.imbalance(tank).abs() < 1e-12 * (into + outof));
}

/// A buffer held full by a tank draining k·level into it, emptied at 0.5/s: the tank drains
/// at 0.5/s (its outlet held back) to level 50, then decays freely; the buffer empties when
/// 10 + 50·(1 − e^(−kτ)) − 0.5·τ = 0 (τ from then): the trapezoid segments (a stock at its
/// bound with a varying rate) and the exact ones agree with the closed form.
#[test]
fn a_held_back_level_dependent_outlet_hands_over_to_free_decay() {
    let k = 0.01;
    let mut n = Network::new(0.0);
    let tank = n.add_stock(Stock::new("tank", 100.0, 0.0, 200.0));
    let buffer = n.add_stock(Stock::new("buffer", 10.0, 0.0, 10.0));
    let sink = n.add_stock(Stock::unbounded("sink"));
    n.add_process(Process {
        name: "outlet".into(),
        flows: vec![
            crate::Flow {
                stock: tank,
                rate: -1.0,
                slope: vec![(tank, -k, 100.0)],
            },
            crate::Flow {
                stock: buffer,
                rate: 1.0,
                slope: vec![(tank, k, 100.0)],
            },
        ],
        trust: Vec::new(),
    });
    n.add_process(Process::constant("use", &[(buffer, -0.5), (sink, 0.5)]));
    let events = n.advance(400.0);
    // The closed form: Newton on f(τ) = 10 + 50(1 − e^(−kτ)) − 0.5τ.
    let mut tau = 150.0;
    for _ in 0..50 {
        let f = 10.0 + 50.0 * (1.0 - creeer_math::det::exp(-k * tau)) - 0.5 * tau;
        let df = 50.0 * k * creeer_math::det::exp(-k * tau) - 0.5;
        tau -= f / df;
    }
    let exact = 100.0 + tau;
    let empty = events.iter().find_map(|e| match *e {
        Event::Empty { stock, time } if stock == buffer => Some(time),
        _ => None,
    });
    let empty = empty.expect("the buffer empties");
    eprintln!(
        "empty at {empty} s (exact {exact}), {} segments",
        n.segments()
    );
    assert!(
        (empty - exact).abs() < 1e-3 * exact,
        "{empty} against {exact}"
    );
    for s in [tank, buffer, sink] {
        assert!(n.imbalance(s).abs() < 1e-12 * 200.0);
    }
}

/// A stock filled by a tank's decaying outflow and drained steadily rises, peaks and falls
/// (B = 100·(1 − e^(−kt)) − 0.3·t, peak 33.9 at 120 s): with a cap of 30 it is full on the
/// way up, though at the segment's end it is far below (the exact search finds a crossing
/// between the ends, at its turning point's side).
#[test]
fn a_stock_that_peaks_between_the_ends_is_caught_at_its_crossing() {
    let k = 0.01;
    let mut n = Network::new(0.0);
    let tank = n.add_stock(Stock::new("tank", 100.0, 0.0, 200.0));
    let b = n.add_stock(Stock::new("b", 0.0, 0.0, 30.0));
    let sink = n.add_stock(Stock::unbounded("sink"));
    n.add_process(Process {
        name: "outlet".into(),
        flows: vec![
            crate::Flow {
                stock: tank,
                rate: -1.0,
                slope: vec![(tank, -k, 100.0)],
            },
            crate::Flow {
                stock: b,
                rate: 1.0,
                slope: vec![(tank, k, 100.0)],
            },
        ],
        trust: Vec::new(),
    });
    n.add_process(Process::constant("drain", &[(b, -0.3), (sink, 0.3)]));
    let events = n.advance(250.0);
    let mut t = 40.0;
    for _ in 0..50 {
        let f = 100.0 * (1.0 - creeer_math::det::exp(-k * t)) - 0.3 * t - 30.0;
        t -= f / (k * 100.0 * creeer_math::det::exp(-k * t) - 0.3);
    }
    let full = events.iter().find_map(|e| match *e {
        Event::Full { stock, time } if stock == b => Some(time),
        _ => None,
    });
    let full = full.expect("b fills on its way up");
    eprintln!("full at {full} (exact {t}), events {events:?}");
    assert!((full - t).abs() < 1e-9 * t, "{full} against {t}");
}
