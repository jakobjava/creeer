//! F6.5 chemistry: 500 furnaces' equilibria at 10 Hz. Each holds hematite reduced by carbon
//! monoxide among nitrogen, over every phase iron and carbon have between 800 and 1400 K
//! (iron's α and γ, wüstite, magnetite, hematite, graphite), its temperature sweeping and
//! fresh gas fed every step, its equilibrium continued from its last.

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_chem::{Fixed, System, equilibrate};
use creeer_materials::species::SpeciesTable;
use rayon::prelude::*;

/// Budget on the minimum-spec CPU (docs/f6-plan.md): 500 reactors' equilibria at 10 Hz,
/// ms per 60 Hz tick (a sixth of them each tick).
const BUDGET_MS: f64 = 0.5;
const NAMES: [&str; 10] = [
    "CO",
    "CO2",
    "O2",
    "N2",
    "C(gr)",
    "Fe(a)",
    "Fe(c)",
    "Fe.947O(cr)",
    "Fe3O4(cr)",
    "Fe2O3(cr)",
];

struct Furnace {
    moles: Vec<f64>,
    t: f64,
    k: usize,
}

/// The furnaces' species and the 500 furnaces: hematite and CO among nitrogen.
fn furnaces() -> (System, Vec<Furnace>) {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &NAMES).expect("the furnace's species");
    let at = |name: &str| sys.index(name).expect("a furnace species");
    let furnaces = (0..500)
        .map(|k| {
            let mut moles = vec![0.0; sys.species.len()];
            moles[at("Fe2O3(cr)")] = 1.0;
            moles[at("CO")] = 2.0;
            moles[at("N2")] = 3.0;
            Furnace {
                moles,
                t: 800.0 + (k % 600) as f64,
                k,
            }
        })
        .collect();
    (sys, furnaces)
}

/// The equilibria of `furnaces` at step `tick` (10 Hz); the most Newton iterations one took.
fn solve(sys: &System, furnaces: &mut [Furnace], tick: u32) -> usize {
    let co = sys.index("CO").expect("CO");
    furnaces
        .par_iter_mut()
        .map(|f| {
            // The furnace heats and cools between 800 and 1400 K; fresh CO is blown in.
            let phase = (f.k + tick as usize) % 1200;
            f.t = 800.0 + if phase < 600 { phase } else { 1200 - phase } as f64;
            let mut feed = f.moles.clone();
            feed[co] += 0.005;
            let eq = equilibrate(
                sys,
                &feed,
                Fixed::Tp {
                    t: f.t,
                    p: 101_325.0,
                },
                Some((&feed, f.t)),
            )
            .expect("a furnace's equilibrium");
            f.moles = eq.moles;
            eq.iterations
        })
        .max()
        .unwrap_or(0)
}

/// The reference load's chemistry (F6.8): each furnace at 10 Hz, a sixth of them a tick.
pub fn load() -> impl FnMut() {
    let (sys, mut furnaces) = furnaces();
    let mut tick = 0u32;
    move || {
        let sixth = furnaces.len() / 6;
        let k = (tick % 6) as usize;
        let end = if k == 5 {
            furnaces.len()
        } else {
            (k + 1) * sixth
        };
        solve(&sys, &mut furnaces[k * sixth..end], tick / 6);
        tick += 1;
    }
}

/// Runs the chemistry benchmark.
pub fn run(report: &mut Report) {
    println!("F6.5 chemistry: 500 furnaces' equilibria at 10 Hz (ms per 60 Hz tick, p50 / p99)");
    let (sys, mut furnaces) = furnaces();
    let mut tick = 0u32;
    let mut most = 0usize;
    let solve = |furnaces: &mut [Furnace], tick: u32| solve(&sys, furnaces, tick);
    // Settled first (the hematite reduced as far as the gas allows).
    for _ in 0..30 {
        most = solve(&mut furnaces, tick);
        tick += 1;
    }
    let t = time_ticks(
        || {
            most = most.max(solve(&mut furnaces, tick));
            tick += 1;
        },
        100,
    );
    let per_tick = p(&t, 50.0) / 6.0;
    println!(
        "  500 equilibria: {:.3} / {:.3} ms every 0.1 s, {per_tick:.3} ms a tick | Newton <= {most} iterations",
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric("chem_500_equilibria_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = per_tick / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= BUDGET_MS,
        format!("500 equilibria at 10 Hz projected to min-spec: {projected:.2} ms a tick <= {BUDGET_MS} ms"),
    );
    println!();
}
