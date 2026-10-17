//! Rates against closed forms: draining, sharing, filling and blocking, exponential decay.

use crate::throttle::fill;
use crate::{Event, Flow, Network, Process, Stock, StockId};

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * b.abs().max(1.0)
}

fn amount(n: &Network, s: StockId) -> f64 {
    n.stock(s).amount
}

/// Every stock's amount is its start plus what came in less what went out.
fn books_close(n: &Network) {
    for k in 0..n.stocks.len() {
        let s = StockId(k);
        let scale = amount(n, s).abs() + n.books(s).0 + n.books(s).1;
        assert!(
            n.imbalance(s).abs() <= 1e-14 * scale.max(1.0),
            "{}: {}",
            n.stock(s).name,
            n.imbalance(s)
        );
    }
}

#[test]
fn a_tank_drains_at_its_rate_and_stops_when_empty_exactly() {
    let mut n = Network::new(0.0);
    let tank = n.add_stock(Stock::new("tank", 100.0, 0.0, 200.0));
    let out = n.add_stock(Stock::unbounded("out"));
    n.add_process(Process::constant("drain", &[(tank, -2.0), (out, 2.0)]));
    assert!(n.advance(30.0).is_empty());
    assert!(close(amount(&n, tank), 40.0, 1e-14));
    let events = n.advance(100.0);
    assert_eq!(events.len(), 1);
    let Event::Empty { stock, time } = events[0] else {
        panic!("{events:?}")
    };
    assert_eq!(stock, tank);
    assert!(close(time, 50.0, 1e-12), "empty at {time}");
    assert!(amount(&n, tank).abs() < 1e-12);
    assert!(close(amount(&n, out), 100.0, 1e-14));
    assert_eq!(n.time(), 100.0);
    books_close(&n);
    // Few segments: one to 30 s, one to the event, one to the end.
    assert!(n.segments() <= 4, "{} segments", n.segments());
}

#[test]
fn consumers_share_a_limited_inflow_in_proportion() {
    let mut n = Network::new(0.0);
    let buffer = n.add_stock(Stock::new("buffer", 0.0, 0.0, 10.0));
    let (a, b) = (
        n.add_stock(Stock::unbounded("a")),
        n.add_stock(Stock::unbounded("b")),
    );
    let source = n.add_stock(Stock::unbounded("source"));
    n.add_process(Process::constant("feed", &[(source, -3.0), (buffer, 3.0)]));
    let pa = n.add_process(Process::constant("A", &[(buffer, -2.0), (a, 2.0)]));
    let pb = n.add_process(Process::constant("B", &[(buffer, -4.0), (b, 4.0)]));
    n.advance(10.0);
    // 3 per second for 6 asked: each at half.
    assert!(close(n.throttle(pa), 0.5, 1e-12) && close(n.throttle(pb), 0.5, 1e-12));
    assert!(close(amount(&n, a), 10.0, 1e-12) && close(amount(&n, b), 20.0, 1e-12));
    assert!(amount(&n, buffer).abs() < 1e-12);
    books_close(&n);
}

#[test]
fn a_full_buffer_holds_back_what_feeds_it() {
    // A miner (2/s) fills an ore bin of 10 that a smelter empties at 1/s: the bin is full at
    // 10 s, then the miner runs at half.
    let mut n = Network::new(0.0);
    let rock = n.add_stock(Stock::unbounded("rock"));
    let bin = n.add_stock(Stock::new("bin", 0.0, 0.0, 10.0));
    let ingots = n.add_stock(Stock::unbounded("ingots"));
    let miner = n.add_process(Process::constant("miner", &[(rock, -2.0), (bin, 2.0)]));
    n.add_process(Process::constant("smelter", &[(bin, -1.0), (ingots, 1.0)]));
    let events = n.advance(100.0);
    let full = events.iter().find_map(|e| match *e {
        Event::Full { stock, time } if stock == bin => Some(time),
        _ => None,
    });
    assert!(
        close(full.expect("the bin fills"), 10.0, 1e-12),
        "{events:?}"
    );
    assert!(close(n.throttle(miner), 0.5, 1e-12));
    assert!(close(amount(&n, ingots), 100.0, 1e-12));
    assert!(close(-amount(&n, rock), 110.0, 1e-12));
    assert!(close(amount(&n, bin), 10.0, 1e-12));
    books_close(&n);
}

#[test]
fn water_filling_gives_what_one_consumer_cannot_take_to_the_others() {
    // Three ask 1 each of a supply of 2; one is held to 0.25 elsewhere: the others share 1.75.
    let phi = fill(&[(0.25, 1.0), (1.0, 1.0), (1.0, 1.0)], 2.0);
    assert!(close(phi, 0.875, 1e-15), "{phi}");
    assert_eq!(fill(&[(1.0, 1.0)], 5.0), 1.0);
    assert_eq!(fill(&[(1.0, 1.0)], 0.0), 0.0);
}

#[test]
fn a_consumer_starved_upstream_leaves_its_share_to_the_others() {
    // Buffer X (empty, fed 1/s) feeds C1 (asks 2/s) only; buffer Y (empty, fed 3/s) feeds
    // C1 (1/s more) and C2 (4/s). C1 runs at 1/2 (X); of Y's 3, C1 takes 0.5 and C2 2.5.
    let mut n = Network::new(0.0);
    let src = n.add_stock(Stock::unbounded("src"));
    let x = n.add_stock(Stock::new("x", 0.0, 0.0, 1.0));
    let y = n.add_stock(Stock::new("y", 0.0, 0.0, 1.0));
    let sink = n.add_stock(Stock::unbounded("sink"));
    n.add_process(Process::constant("feed x", &[(src, -1.0), (x, 1.0)]));
    n.add_process(Process::constant("feed y", &[(src, -3.0), (y, 3.0)]));
    let c1 = n.add_process(Process::constant(
        "C1",
        &[(x, -2.0), (y, -1.0), (sink, 3.0)],
    ));
    let c2 = n.add_process(Process::constant("C2", &[(y, -4.0), (sink, 4.0)]));
    n.advance(1.0);
    assert!(close(n.throttle(c1), 0.5, 1e-12), "{}", n.throttle(c1));
    assert!(close(n.throttle(c2), 0.625, 1e-12), "{}", n.throttle(c2));
    books_close(&n);
}

#[test]
fn a_closed_empty_loop_does_not_run() {
    let mut n = Network::new(0.0);
    let (a, b) = (
        n.add_stock(Stock::new("a", 0.0, 0.0, 5.0)),
        n.add_stock(Stock::new("b", 0.0, 0.0, 5.0)),
    );
    let p = n.add_process(Process::constant("a→b", &[(a, -1.0), (b, 1.0)]));
    let q = n.add_process(Process::constant("b→a", &[(b, -1.0), (a, 1.0)]));
    n.advance(10.0);
    assert_eq!((amount(&n, a), amount(&n, b)), (0.0, 0.0));
    // Either may run as far as the other feeds it: none, from empty (the largest fixed point
    // here is any equal pair; from 1 down it keeps both at 1, moving nothing net).
    assert!(close(n.throttle(p), n.throttle(q), 1e-12));
}

#[test]
fn an_outflow_proportional_to_the_level_decays_exponentially() {
    // Outflow k·level (k = 0.01/s), measured at 100: rate −1 with slope −0.01.
    let mut n = Network::new(0.0);
    let tank = n.add_stock(Stock::new("tank", 100.0, 0.0, 1000.0));
    let out = n.add_stock(Stock::unbounded("out"));
    n.add_process(Process {
        name: "outlet".into(),
        flows: vec![
            Flow {
                stock: tank,
                rate: -1.0,
                slope: vec![(tank, -0.01, 100.0)],
            },
            Flow {
                stock: out,
                rate: 1.0,
                slope: vec![(tank, 0.01, 100.0)],
            },
        ],
        trust: Vec::new(),
    });
    n.advance(300.0);
    let exact = 100.0 * creeer_math::det::exp(-3.0f64);
    eprintln!(
        "exponential: {} segments, relative error {:.2e}",
        n.segments(),
        amount(&n, tank) / exact - 1.0
    );
    assert!(
        close(amount(&n, tank), exact, 1e-12),
        "{} against {exact}",
        amount(&n, tank)
    );
    assert!(close(amount(&n, tank) + amount(&n, out), 100.0, 1e-14));
    books_close(&n);
    // Exact: one segment (the matrix exponential), not ln(20)/1 % of trapezoids.
    assert!(n.segments() <= 2, "{} segments", n.segments());
}

#[test]
fn a_year_of_a_factory_closes_its_books() {
    let mut n = Network::new(0.0);
    let ore = n.add_stock(Stock::new("ore", 1e6, 0.0, 2e6));
    let bin = n.add_stock(Stock::new("bin", 0.0, 0.0, 500.0));
    let metal = n.add_stock(Stock::new("metal", 0.0, 0.0, 1e5));
    let slag = n.add_stock(Stock::unbounded("slag"));
    n.add_process(Process::constant("mine", &[(ore, -0.03), (bin, 0.03)]));
    n.add_process(Process::constant(
        "smelt",
        &[(bin, -0.02), (metal, 0.015), (slag, 0.005)],
    ));
    n.add_process(Process::constant("ship", &[(metal, -0.01)]));
    let events = n.advance(365.25 * 86_400.0);
    eprintln!("a year: {} segments, {events:?}", n.segments());
    books_close(&n);
    // Mass: what left the ore went to the bin, metal, slag and the ships.
    let shipped = n.books(metal).1;
    let total = amount(&n, bin) + amount(&n, metal) + amount(&n, slag) + shipped;
    assert!(close(1e6 - amount(&n, ore), total, 1e-12), "{events:?}");
    assert!(n.segments() < 20, "{} segments", n.segments());
}
