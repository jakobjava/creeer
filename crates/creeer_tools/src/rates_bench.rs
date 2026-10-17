//! F7.3 abstract mode (ADR-0020): 10,000 machines of five stocks each (50,000 stocks), each
//! its own network, advanced through a day of game time at once.
//!
//! Each machine: a mine draws from a deposit into an ore bin; a smelter turns ore into metal
//! (the bin fills when the smelter is slower, then holds the mine back); a cooling tank
//! drains at a rate that falls with its level (a linear dependence: segments sized to it);
//! shipping takes metal away. Rates differ from machine to machine.

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report};
use creeer_rates::{Flow, Network, Process, Stock, StockId, advance_all};
use std::time::Instant;

/// A day of 10,000 machines on min-spec (ms).
const BUDGET_MS: f64 = 10.0;

fn machine(k: u32) -> Network {
    let x = f64::from(k % 97) / 97.0;
    let mut n = Network::new(0.0);
    let deposit = n.add_stock(Stock::new("deposit", 5e4 * (1.0 + x), 0.0, 1e9));
    let bin = n.add_stock(Stock::new("ore bin", 0.0, 0.0, 2000.0));
    let metal = n.add_stock(Stock::new("metal", 0.0, 0.0, 5e3));
    let tank = n.add_stock(Stock::new("coolant", 40.0, 0.0, 50.0));
    let drain = n.add_stock(Stock::unbounded("drain"));
    let mine = 0.5 + 0.3 * x;
    n.add_process(Process::constant("mine", &[(deposit, -mine), (bin, mine)]));
    n.add_process(Process::constant(
        "smelt",
        &[(bin, -0.6), (metal, 0.4), (drain, 0.2)],
    ));
    n.add_process(Process::constant("ship", &[(metal, -0.3 * (0.5 + x))]));
    // Outflow 1e-4/s of the level, measured at 40.
    let k = 1e-4 * (1.0 + x);
    n.add_process(Process {
        name: "coolant".into(),
        flows: vec![
            Flow {
                stock: tank,
                rate: -k * 40.0,
                slope: vec![(tank, -k, 40.0)],
            },
            Flow {
                stock: drain,
                rate: k * 40.0,
                slope: vec![(tank, k, 40.0)],
            },
        ],
        trust: Vec::new(),
    });
    n
}

pub fn run(report: &mut Report) {
    println!("F7.3 abstract mode: 10,000 machines (50,000 stocks) through a day (ms)");
    let mut machines: Vec<Network> = (0..10_000).map(machine).collect();
    // Warm up the thread pool (a server's is running), on copies.
    let mut warm: Vec<Network> = machines.iter().take(1000).cloned().collect();
    advance_all(&mut warm, 3600.0);
    let start = Instant::now();
    let events = advance_all(&mut machines, 86_400.0);
    let ms = start.elapsed().as_secs_f64() * 1e3;
    let segments: u64 = machines.iter().map(Network::segments).sum();
    let count: usize = events.iter().map(Vec::len).sum();
    let worst = machines
        .iter()
        .flat_map(|n| {
            // Relative to what passed through the stock.
            (0..5).map(move |s| {
                let (into, out) = n.books(StockId(s));
                let scale = into + out + n.stock(StockId(s)).amount.abs();
                n.imbalance(StockId(s)).abs() / scale.max(1.0)
            })
        })
        .fold(0.0, f64::max);
    println!(
        "  a day: {ms:.2} ms | {segments} segments ({:.1} a machine), {count} events | worst books {worst:.1e}",
        segments as f64 / 10_000.0
    );
    report.metric("rates_10k_machines_day_ms", ms, Better::Lower);
    let projected = ms / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= BUDGET_MS && worst < 1e-12,
        format!("10,000 machines through a day projected to min-spec: {projected:.2} ms <= {BUDGET_MS} ms (books {worst:.0e})"),
    );
}
