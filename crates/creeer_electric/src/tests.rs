//! Circuits against circuit theory, and the ledger's closure.

use crate::*;
use creeer_materials::MaterialTable;
use creeer_math::det;
use std::sync::Arc;

const TICK: f64 = 1.0 / 60.0;

fn steps(c: &mut Circuit, dt: f64, n: usize) -> StepReport {
    let mut last = StepReport::default();
    for _ in 0..n {
        last = c.step(dt);
    }
    last
}

fn closes(c: &Circuit) -> f64 {
    c.ledger().relative_imbalance(c.stored())
}

/// 10 V across 1 kΩ and 3 kΩ: 7.5 V between them, 2.5 mA, 25 mW, all of it heat.
#[test]
fn voltage_divider() {
    let mut c = Circuit::new();
    let (top, mid) = (c.node(), c.node());
    let source = c.voltage_source(top, GROUND, 10.0);
    let upper = c.resistor(top, mid, 1000.0);
    c.resistor(mid, GROUND, 3000.0);
    let report = c.step(TICK);
    assert_eq!((report.iterations, report.factorisations), (1, 1));
    // To GMIN's 1 TΩ leak from the node (it takes 5.6 nV; booked as dissipation).
    assert!((c.voltage(mid) - 7.5).abs() < 1e-8, "{}", c.voltage(mid));
    assert!((c.current(upper) - 2.5e-3).abs() < 1e-11);
    // The source drives 2.5 mA out of its + terminal: through it from + to − is −2.5 mA.
    // GMIN leaks 17.5 pA from the two nodes.
    assert!((c.current(source) + 2.5e-3).abs() < 1e-10);
    steps(&mut c, TICK, 59);
    let (supplied, dissipated, _, _) = c.ledger().totals();
    // GMIN leaks 0.16 nJ in that second.
    assert!(
        (supplied - 0.025).abs() < 2e-10 && (dissipated - supplied).abs() < 1e-15,
        "{supplied} {dissipated}"
    );
    assert!(closes(&c) < 1e-14);
}

/// 5 V charging 1 mF through 1 kΩ (τ = 1 s), stepped at 10 ms: v = V·(1 − e^(−t/τ)) to
/// BDF2's accuracy; the resistor turns half the energy into heat, the capacitor keeps the
/// other half (½CV² each, the classic result).
#[test]
fn rc_time_constant() {
    let mut c = Circuit::new();
    let (top, mid) = (c.node(), c.node());
    c.voltage_source(top, GROUND, 5.0);
    let r = c.resistor(top, mid, 1000.0);
    c.capacitor(mid, GROUND, 1e-3);
    let dt = 0.01;
    for k in 1..=1000 {
        c.step(dt);
        let t = f64::from(k) * dt;
        let exact = 5.0 * (1.0 - det::exp(-t));
        // The first two steps are backward Euler's: each's local error is (dt/τ)²/2·V = 2.5e-4 V.
        assert!(
            (c.voltage(mid) - exact).abs() < 6e-4,
            "t = {t}: {} vs {exact}",
            c.voltage(mid)
        );
    }
    // Ten time constants on: 5·(1 − e⁻¹⁰) V.
    let charged = 5.0 * (1.0 - det::exp(-10.0));
    let half = 0.5 * 1e-3 * charged * charged;
    assert!(
        (c.stored() - half).abs() < 1e-6 * half,
        "{} vs {half}",
        c.stored()
    );
    // The resistor's heat: ∫(V − v)²/R·dt = ½·C·V²·(1 − e^(−2t/τ)).
    let heat = 0.5 * 1e-3 * 25.0 * (1.0 - det::exp(-20.0));
    let took = c.take_heat(r);
    assert!(
        (took / heat - 1.0).abs() < 2e-3,
        "the resistor's heat {took} vs {heat}"
    );
    assert!(closes(&c) < 1e-12);
}

/// The same with τ = 1 µs at a 60 Hz tick: BDF2 settles it in a step, without ringing (the
/// trapezoidal rule's −1 would flip it every tick), and the energy the tick cannot resolve
/// is still booked: half to the capacitor, the rest dissipated.
#[test]
fn a_transient_faster_than_the_tick_settles() {
    let mut c = Circuit::new();
    let (top, mid) = (c.node(), c.node());
    c.voltage_source(top, GROUND, 5.0);
    c.resistor(top, mid, 1.0);
    c.capacitor(mid, GROUND, 1e-6);
    // Backward Euler's steps each close all but RC/h (6·10⁻⁵) of the gap; BDF2 then goes on by
    // that factor, overshooting by at most (RC/h)² of the jump (10⁻⁸ V).
    let mut gaps = Vec::new();
    for _ in 0..5 {
        c.step(TICK);
        let gap = 5.0 - c.voltage(mid);
        assert!(gap >= -1e-8, "rang past: {}", c.voltage(mid));
        gaps.push(gap);
    }
    assert!(
        gaps[0] < 6.1e-5 * 5.0 && gaps[2] < 1e-9 && gaps[4] < 1e-11,
        "{gaps:?}"
    );
    let (supplied, dissipated, _, _) = c.ledger().totals();
    // The source gave C·V² but for BDF2's restart term ½·C·(v₂ − v₁)·V (0.75 nJ: the module docs).
    assert!(
        (supplied - 25e-6).abs() < 1e-9 && (dissipated - (supplied - c.stored())).abs() < 1e-15,
        "{supplied} {dissipated}"
    );
    // To the LU's backward error: Kirchhoff's laws hold to ε·|A|·|x| ≈ 5·10⁻¹⁶ A, here 2·10⁻¹⁶ J.
    assert!(closes(&c) < 1e-10, "{:e}", closes(&c));
}

/// A series RLC ringing down (L = 10 mH, C = 10 µF, R = 2 Ω): f₀ = 1/(2π√(LC)) = 503.3 Hz,
/// Q = √(L/C)/R = 15.8, stepped at 2 µs (resolved).
#[test]
fn rlc_resonance() {
    let (l, cap, r) = (10e-3, 10e-6, 2.0);
    let mut c = Circuit::new();
    let (a, b) = (c.node(), c.node());
    let switch = c.add(Kind::Switch, a, GROUND);
    c.capacitor(a, GROUND, cap);
    c.inductor(a, b, l);
    c.resistor(b, GROUND, r - CONTACT);
    // Charge the capacitor to 10 V first (the switch open, a source through 1 Ω), then let
    // it ring: the source removed by opening its own switch.
    let top = c.node();
    c.voltage_source(top, GROUND, 10.0);
    let feed = c.add(Kind::Switch, top, a);
    c.set_closed(switch, false);
    steps(&mut c, 1e-4, 100);
    c.set_closed(feed, false);
    let dt = 2e-6;
    let (mut crossings, mut peaks, mut last) = (Vec::new(), Vec::new(), c.voltage(a));
    let mut rising = false;
    for k in 1..=20_000 {
        c.step(dt);
        let v = c.voltage(a);
        if last < 0.0 && v >= 0.0 {
            crossings.push(f64::from(k) * dt - dt * v / (v - last));
        }
        if rising && v < last && last > 0.0 {
            peaks.push(last);
        }
        rising = v > last;
        last = v;
    }
    let period = (crossings[crossings.len() - 1] - crossings[0]) / (crossings.len() - 1) as f64;
    let f0 = 1.0 / (2.0 * std::f64::consts::PI * (l * cap).sqrt());
    let q0 = (l / cap).sqrt() / r;
    let damped = f0 * (1.0 - 1.0 / (4.0 * q0 * q0)).sqrt();
    assert!(
        (1.0 / period / damped - 1.0).abs() < 2e-3,
        "{} Hz vs {damped}",
        1.0 / period
    );
    // Q from the decay: peaks fall by e^(−π/Q) a cycle.
    let q = std::f64::consts::PI * (peaks.len() - 1) as f64
        / det::ln(peaks[0] / peaks[peaks.len() - 1]);
    let exact = (l / cap).sqrt() / r;
    assert!((q / exact - 1.0).abs() < 0.02, "Q {q} vs {exact}");
    assert!(closes(&c) < 1e-9);
}

/// A copper wire's resistance follows its temperature (CRC: ρ 1.69e-8 at 20 °C, 2.23e-8 Ω·m at
/// 100 °C): heated, it carries less.
#[test]
fn a_wire_heats_and_resists() {
    let copper = MaterialTable::reference()
        .get("copper_c11000")
        .unwrap()
        .clone();
    let mut c = Circuit::new();
    let top = c.node();
    c.voltage_source(top, GROUND, 1.0);
    let wire = c.add(
        Kind::Wire {
            resistivity: Arc::new(copper.resistivity.clone()),
            length: 10.0,
            area: 1e-6,
        },
        top,
        GROUND,
    );
    c.step(TICK);
    let cold = c.current(wire);
    assert!((cold - 1.0 / (1.69e-8 * 10.0 / 1e-6)).abs() < 1e-9 * cold);
    c.set_temperature(wire, 373.15);
    c.step(TICK);
    assert!((c.current(wire) / cold - 1.69 / 2.23).abs() < 1e-9);
    assert!(c.take_heat(wire) > 0.0);
}

/// Islands step in parallel and give the same bits as one by one.
#[test]
fn deterministic() {
    let run = || {
        let mut c = Circuit::new();
        let mut probes = Vec::new();
        for k in 0..40 {
            let (top, mid) = (c.node(), c.node());
            c.voltage_source(top, GROUND, 5.0 + f64::from(k));
            c.resistor(top, mid, 100.0);
            c.diode(mid, GROUND, Diode::RED_LED);
            c.capacitor(mid, GROUND, 1e-3);
            probes.push(mid);
        }
        steps(&mut c, TICK, 30);
        probes
            .iter()
            .map(|&n| c.voltage(n).to_bits())
            .collect::<Vec<_>>()
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    assert_eq!(run(), pool.install(run));
}

/// A static circuit falls asleep and keeps booking its losses; changing it wakes it.
#[test]
fn a_steady_circuit_sleeps_and_still_dissipates() {
    let mut c = Circuit::new();
    let top = c.node();
    let source = c.voltage_source(top, GROUND, 12.0);
    c.resistor(top, GROUND, 6.0);
    steps(&mut c, TICK, 40);
    assert_eq!(c.step(TICK).solved, 0, "asleep");
    let (supplied, dissipated, _, _) = c.ledger().totals();
    assert!((supplied - 24.0 * 41.0 * TICK).abs() < 1e-9 && (dissipated - supplied).abs() < 1e-12);
    c.set_source(source, 6.0);
    assert_eq!(c.step(TICK).solved, 1, "woken");
    assert!((c.voltage(top) - 6.0).abs() < 1e-12);
}
