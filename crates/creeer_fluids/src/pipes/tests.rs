//! Pipe flow against its classics: the Moody chart, Hagen–Poiseuille, Torricelli, a pump
//! on its system curve, a check valve, the books.

use super::friction::{colebrook, darcy};
use super::{LinkKind, Liquid, Network, NodeKind};
use creeer_math::det;

const G: f64 = 9.80665;
const WATER: Liquid = Liquid::WATER_20C;

/// The Moody chart's points (L. F. Moody, Trans. ASME 66, 1944; the Colebrook equation):
/// smooth pipe at Re 10⁵ and 10⁶, ε/D 10⁻³ at 10⁵ and 10⁷, the fully rough limit
/// 1/√f = −2·log₁₀(ε/3.7D) (von Kármán) at ε/D 0.01; laminar 64/Re; the transition smooth.
#[test]
fn friction_follows_the_moody_chart() {
    for (re, rr, chart) in [
        (1e5, 0.0, 0.0180),
        (1e6, 0.0, 0.0116),
        (1e5, 1e-3, 0.0222),
        (1e7, 1e-3, 0.0196),
    ] {
        let f = colebrook(re, rr);
        assert!(
            (f / chart - 1.0).abs() < 0.01,
            "Re {re}, ε/D {rr}: f {f}, chart {chart}"
        );
    }
    let rough = 1.0 / det::pow(-2.0 * det::log10(0.01 / 3.7), 2.0);
    assert!((colebrook(1e9, 0.01) / rough - 1.0).abs() < 1e-3);
    assert!((darcy(1000.0, 0.0) - 0.064).abs() < 1e-15);
    // Continuous across the transition.
    for re in [2000.0, 4000.0] {
        let jump = (darcy(re * (1.0 + 1e-9), 1e-4) - darcy(re * (1.0 - 1e-9), 1e-4)).abs();
        assert!(jump < 1e-9, "Re {re}: jumps by {jump}");
    }
}

/// Laminar flow through a 5 mm pipe 10 m long under 0.1 m of head: Q = π·D⁴·ρ·g·Δh/(128·μ·L)
/// (Hagen–Poiseuille), exactly.
#[test]
fn laminar_flow_is_hagen_poiseuille() {
    let mut net = Network::new(WATER, G);
    let (a, b) = (
        net.node(0.0, NodeKind::Reservoir, 0.1),
        net.node(0.0, NodeKind::Reservoir, 0.0),
    );
    let pipe = net.link(
        a,
        b,
        LinkKind::Pipe {
            length: 10.0,
            diameter: 0.005,
            roughness: 0.0,
            minor: 0.0,
            check: false,
        },
    );
    assert!(net.step(1.0).converged);
    let exact = std::f64::consts::PI * det::pow(0.005, 4.0) * WATER.density * G * 0.1
        / (128.0 * WATER.viscosity * 10.0);
    assert!(
        (net.links[pipe].flow / exact - 1.0).abs() < 1e-9,
        "{} m³/s, {exact}",
        net.links[pipe].flow
    );
}

/// A tank of 1 m² draining through a 50 mm orifice (no loss but its jet's v²/2g): the
/// level falls as √H = √H₀ − (a/A)·√(g/2)·t (Torricelli), 2 m to 0.5 m in 162.6 s.
#[test]
fn a_tank_drains_as_torricelli() {
    let mut net = Network::new(WATER, G);
    let tank = net.node(
        0.0,
        NodeKind::Tank {
            area: 1.0,
            floor: 0.0,
        },
        2.0,
    );
    let out = net.node(0.0, NodeKind::Reservoir, 0.0);
    net.link(
        tank,
        out,
        LinkKind::Valve {
            diameter: 0.05,
            open: 1.0,
            opening: 1.0,
        },
    );
    let a = 0.25 * std::f64::consts::PI * 0.05 * 0.05;
    let dt = 0.01;
    let mut t = 0.0;
    while net.nodes[tank].head > 0.5 {
        assert!(net.step(dt).converged);
        t += dt;
    }
    let exact = (det::pow(2.0, 0.5) - det::pow(0.5, 0.5)) / (a * det::pow(G / 2.0, 0.5));
    assert!(
        (t - exact).abs() < 0.01 * exact,
        "{t} s, Torricelli {exact}"
    );
    assert!(net.ledger().relative_imbalance(net.stored()) < 1e-9);
}

/// A pump (shut-off 40 m, r = 2000 m per (m³/s)²) lifting water 20 m through 200 m of rough
/// 100 mm pipe settles where its curve meets the system's: H₀ − r·Q² = Δz + K·Q².
#[test]
fn a_pump_meets_its_system_curve() {
    let mut net = Network::new(WATER, G);
    let (low, high) = (
        net.node(0.0, NodeKind::Reservoir, 0.0),
        net.node(20.0, NodeKind::Reservoir, 20.0),
    );
    let mid = net.node(0.0, NodeKind::Junction { demand: 0.0 }, 0.0);
    let pump = net.link(
        low,
        mid,
        LinkKind::Pump {
            shutoff: 40.0,
            resistance: 2000.0,
            speed: 1.0,
            best_flow: 0.05,
            best_efficiency: 0.75,
        },
    );
    let pipe = LinkKind::Pipe {
        length: 200.0,
        diameter: 0.1,
        roughness: 1e-3,
        minor: 0.0,
        check: false,
    };
    net.link(mid, high, pipe);
    for _ in 0..3 {
        assert!(net.step(1.0).converged);
    }
    let q = net.links[pump].flow;
    // The system's K at this flow's friction factor.
    let area = 0.25 * std::f64::consts::PI * 0.01;
    let re = WATER.density * q / area * 0.1 / WATER.viscosity;
    let k = darcy(re, 0.01) * 200.0 / 0.1 / (2.0 * G * area * area);
    let exact = det::pow((40.0 - 20.0) / (2000.0 + k), 0.5);
    assert!(
        (q / exact - 1.0).abs() < 1e-6,
        "{q} m³/s, curves meet at {exact}"
    );
    // Stopped, the check valve in the pump holds the column: nothing flows back.
    net.links[pump].kind = LinkKind::Pump {
        shutoff: 40.0,
        resistance: 2000.0,
        speed: 0.0,
        best_flow: 0.05,
        best_efficiency: 0.75,
    };
    for _ in 0..3 {
        assert!(net.step(1.0).converged);
    }
    assert_eq!(net.links[pump].flow, 0.0);
    assert!(!net.links[pump].open);
}

/// A pump filling a tank while a consumer draws from it: every joule booked (pump work in,
/// friction and pump losses out as heat, consumers' water's energy out, the tank's
/// potential energy held).
#[test]
fn a_network_keeps_its_books() {
    let mut net = Network::new(WATER, G);
    let well = net.node(-5.0, NodeKind::Reservoir, -5.0);
    let tank = net.node(
        10.0,
        NodeKind::Tank {
            area: 4.0,
            floor: 10.0,
        },
        11.0,
    );
    let tap = net.node(0.0, NodeKind::Junction { demand: 0.002 }, 0.0);
    net.link(
        well,
        tank,
        LinkKind::Pump {
            shutoff: 30.0,
            resistance: 5000.0,
            speed: 1.0,
            best_flow: 0.03,
            best_efficiency: 0.7,
        },
    );
    net.link(
        tank,
        tap,
        LinkKind::Pipe {
            length: 50.0,
            diameter: 0.05,
            roughness: 1e-4,
            minor: 2.0,
            check: false,
        },
    );
    for _ in 0..600 {
        assert!(net.step(0.5).converged);
    }
    assert!(net.nodes[tank].head > 11.0, "the pump fills it");
    assert!(
        net.ledger().relative_imbalance(net.stored()) < 1e-9,
        "{}",
        net.ledger().relative_imbalance(net.stored())
    );
}
