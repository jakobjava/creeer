//! Networks delivering what went into them: hot and cold water mixed at a junction and
//! carried down a pipe at the temperature their enthalpies give; a reactor's gas through a
//! pipeline's line pack; the books.

use super::gas::{Gas, GasKind, GasNetwork};
use super::{LinkKind, Liquid, Network, NodeKind, of_mixture, temperature};
use creeer_chem::System;
use creeer_materials::species::SpeciesTable;

const G: f64 = 9.80665;

fn pipe(length: f64, diameter: f64) -> LinkKind {
    LinkKind::Pipe {
        length,
        diameter,
        roughness: 1e-5,
        minor: 0.0,
        check: false,
    }
}

/// Water at 80 °C and at 20 °C from two reservoirs meet at a junction and run down 200 m of
/// pipe to a tap: the tap's water arrives after the pipe's transit, at the temperature of the
/// mixed enthalpy (the flows' weighted mean, ≈ 50 °C at equal flows, water's cp varying
/// 0.3 % between); every kilogram and joule booked.
#[test]
fn hot_and_cold_mix_and_arrive_at_their_temperature() {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &["H2O(L)"]).unwrap();
    let hot = of_mixture(&sys, &[1.0], 353.15);
    let cold = of_mixture(&sys, &[1.0], 293.15);
    let mut net = Network::new(Liquid::WATER_20C, G);
    let (h, c) = (
        net.node(0.0, NodeKind::Reservoir, 10.0),
        net.node(0.0, NodeKind::Reservoir, 10.0),
    );
    let joint = net.node(0.0, NodeKind::Junction { demand: 0.0 }, 0.0);
    let tap = net.node(0.0, NodeKind::Junction { demand: 0.002 }, 0.0);
    net.link(h, joint, pipe(10.0, 0.05));
    net.link(c, joint, pipe(10.0, 0.05));
    let main = net.link(joint, tap, pipe(200.0, 0.05));
    let mut tracer = net.tracer(&cold);
    tracer.set(h, &hot);
    let transit = net.links[main].flow;
    assert_eq!(transit, 0.0, "no flow before the first step");
    let mut t = 0.0;
    let mut arrived = None;
    while t < 400.0 {
        assert!(net.step(1.0).converged);
        net.carry(&mut tracer, 1.0);
        t += 1.0;
        let at = temperature(&sys, tracer.content(tap), 300.0);
        if arrived.is_none() && at > 300.0 {
            arrived = Some(t);
        }
    }
    // The hot feeder's 19.6 l at 1 l/s (19.6 s), then the main's 393 l at 2 l/s (196 s).
    let per_metre = 0.25 * std::f64::consts::PI * 0.05 * 0.05;
    let transit = per_metre * 10.0 / 0.001 + per_metre * 200.0 / 0.002;
    let arrived = arrived.expect("the warm water arrived");
    assert!(
        (arrived - transit).abs() <= 1.0,
        "arrived at {arrived} s, transit {transit} s"
    );
    let mixed: Vec<f64> = hot.iter().zip(&cold).map(|(x, y)| 0.5 * (x + y)).collect();
    let exact = temperature(&sys, &mixed, 320.0);
    let at = temperature(&sys, tracer.content(tap), 320.0);
    assert!((at - exact).abs() < 1e-6, "{at} K, mixed {exact} K");
    assert!((exact - 323.15).abs() < 0.2, "{exact} K");
    for (q, held) in tracer.held().iter().enumerate() {
        let flows: Vec<f64> = net.links.iter().map(|l| l.flow).collect();
        let off = tracer.ledger(q).relative_imbalance(*held);
        assert!(
            off < 1e-9,
            "quantity {q}: {off} {:?} {held} {flows:?}",
            tracer.ledger(q).totals()
        );
    }
}

/// A reactor's steam and hydrogen at 900 K fed into a pipeline of ten sections holding
/// methane: the last section's gas turns over as a train of stirred stores would (half
/// turned over near the line pack's residence time); content and books right. (The
/// pipeline's hydraulics take its gas as methane; a stream's own is passive in it.)
#[test]
fn a_pipeline_delivers_what_the_reactor_made() {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &["CH4", "H2", "H2O"]).unwrap();
    let methane = of_mixture(&sys, &[1.0, 0.0, 0.0], 288.15);
    let made = of_mixture(&sys, &[0.0, 1.0, 1.0], 900.0);
    let gas = Gas::METHANE_15C;
    let mut net = GasNetwork::new(gas);
    let (d, l, sections) = (0.3, 20_000.0, 10);
    let volume = 0.25 * std::f64::consts::PI * d * d * l / f64::from(sections);
    let inlet = net.node(GasKind::Supply, 40e5);
    let mut chain = vec![inlet];
    for k in 1..sections {
        let (p1, p2) = (40e5 * 40e5, 30e5 * 30e5);
        let p: f64 = (p1 - (p1 - p2) * f64::from(k) / f64::from(sections)).sqrt();
        chain.push(net.node(
            GasKind::Store {
                volume,
                demand: 0.0,
            },
            p,
        ));
    }
    let outlet = net.node(GasKind::Supply, 30e5);
    chain.push(outlet);
    for w in chain.windows(2) {
        net.pipe(w[0], w[1], l / f64::from(sections), d, 1e-5, None);
    }
    for _ in 0..50 {
        assert!(net.step(60.0));
    }
    let mut tracer = net.tracer(&methane);
    tracer.set(inlet, &made);
    let flow = net.pipes[0].flow;
    let pack = net.stored();
    let residence = pack / flow;
    let last = chain[sections as usize - 1];
    let hydrogen = |tr: &super::content::Tracer| tr.content(last)[1] / made[1];
    let mut t = 0.0;
    while t < residence {
        assert!(net.step(10.0));
        net.carry(&mut tracer, 10.0);
        t += 10.0;
    }
    let half = hydrogen(&tracer);
    assert!(
        (0.3..0.7).contains(&half),
        "{half} turned over at the residence time"
    );
    while t < 6.0 * residence {
        assert!(net.step(10.0));
        net.carry(&mut tracer, 10.0);
        t += 10.0;
    }
    let all = hydrogen(&tracer);
    assert!(all > 0.999, "{all}");
    let at = temperature(&sys, tracer.content(last), 600.0);
    assert!((at - 900.0).abs() < 1.0, "{at} K");
    for (q, held) in tracer.held().iter().enumerate() {
        assert!(
            tracer.ledger(q).relative_imbalance(*held) < 1e-9,
            "quantity {q}"
        );
    }
}
